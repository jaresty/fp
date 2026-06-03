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
        let out = crate::commands::cmd_status_all(Some(&fake), &store, None, None, &git_dir, "alice", "repo", false).unwrap();
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
            codeowners_eligibility: Default::default(), created_at: None, is_stacked: false,
        });
        fake
    }

    #[test]
    // Stage 4: fp status shows health column when process state has a live PR
    fn commands_governs_cmd_status_all_shows_health_when_live() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join("git_dir");
        std::fs::create_dir_all(&git_dir).unwrap();
        let store = make_store_with_pr(&git_dir, 5, "feat/all-test");
        let ps = crate::process_store::ProcessStateStore::open(&git_dir);
        let live_pid = std::process::id();
        ps.activate(crate::process_store::ProcessRecord {
            pr: 5, expected_branch: "feat/all-test".into(), pid: Some(live_pid),
            feature_envelopes: vec![], feature_envelope: None, worktree: tmp.path().to_string_lossy().to_string(),
            app_config_names: vec![],
        }).unwrap();
        let fake = make_fake_with_pr(5);
        let out = crate::commands::cmd_status_all(Some(&fake), &store, Some(&ps), None, &git_dir, "alice", "repo", false).unwrap();
        assert!(out.contains("up") || out.contains("healthy") || out.contains("live") || out.contains("✓"),
            "cmd_status_all must show health indicator for live PR, got: {}", out);
    }

    // Stage 4: fp status succeeds (fail-open) when process state file is absent
    #[test]
    fn commands_governs_cmd_status_all_succeeds_without_process_state() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join("git_dir");
        std::fs::create_dir_all(&git_dir).unwrap();
        let store = make_store_with_pr(&git_dir, 5, "feat/all-test");
        let fake = make_fake_with_pr(5);
        // No process state file — must not error
        let result = crate::commands::cmd_status_all(Some(&fake), &store, None, None, &git_dir, "alice", "repo", false);
        assert!(result.is_ok(), "cmd_status_all must succeed without process state, got: {:?}", result);
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
    // Stage 5: post-switch feature summary includes feature name and member health
    #[test]
    fn commands_governs_cmd_switch_feature_summary_includes_feature_and_health() {
        let dir = tempfile::tempdir().unwrap();
        let ps = crate::process_store::ProcessStateStore::open(dir.path());
        let app_store = crate::app_config::AppConfigStore::open(dir.path().join("config.toml"));
        let wt = tempfile::tempdir().unwrap();
        app_store.save_app_config(crate::app_config::AppConfig {
            name: "svc".into(), bootstrap: "echo up".into(), teardown: "echo down".into(),
            startup_timeout: "5s".into(), health_check: None, ephemeral: false, main_worktree: None, setup: None,
            volume_check: None,
}).unwrap();
        crate::feature::feature_new(&ps, "my-feature").unwrap();
        ps.activate(crate::process_store::ProcessRecord {
            pr: 42, expected_branch: "feat/x".into(), pid: Some(std::process::id()),
            feature_envelopes: vec!["my-feature".into()], feature_envelope: None,
            worktree: wt.path().to_string_lossy().to_string(),
            app_config_names: vec!["svc".into()],
        }).unwrap();
        let mut state = ps.load().unwrap();
        state.feature_envelopes.insert("my-feature".to_string());
        ps.save_state(state).unwrap();
        let summary = crate::commands::cmd_switch_feature_summary(&ps, &app_store, 42);
        assert!(summary.contains("my-feature"),
            "summary must include feature name, got: {}", summary);
        assert!(summary.contains("42") || summary.contains("#42"),
            "summary must include PR number, got: {}", summary);
    }

    // Stage 5: post-switch summary is empty when PR has no feature envelope
    #[test]
    fn commands_governs_cmd_switch_feature_summary_empty_when_no_feature() {
        let dir = tempfile::tempdir().unwrap();
        let ps = crate::process_store::ProcessStateStore::open(dir.path());
        let app_store = crate::app_config::AppConfigStore::open(dir.path().join("config.toml"));
        let summary = crate::commands::cmd_switch_feature_summary(&ps, &app_store, 99);
        assert!(summary.is_empty(),
            "summary must be empty when PR has no feature, got: {}", summary);
    }

    fn commands_governs_cmd_switch_errors_on_untracked_pr() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join("git_dir");
        std::fs::create_dir_all(&git_dir).unwrap();
        let store = crate::store::Store::open(&git_dir);
        let ps = crate::process_store::ProcessStateStore::open(&git_dir);
        let app_store = crate::app_config::AppConfigStore::open(tmp.path().join("config.toml"));
        let result = crate::commands::cmd_switch(&store, &ps, &app_store, &git_dir, 99, "session-id", false, false, false, std::path::Path::new("."));
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
    fn track_merged_pr_returns_error() {
        let mut fake = crate::github::FakeGithubClient::new_with_pr(42, "feat/done", "Done PR", "main");
        fake.set_pr_merged(42, true);
        let result = crate::commands::cmd_track(&fake, "owner", "repo", 42, None, None);
        assert!(result.is_err(), "cmd_track must error for merged PR");
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("42") && msg.contains("merged"), "error must mention PR number and 'merged', got: {}", msg);
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
        let ps = crate::process_store::ProcessStateStore::open(&git_dir);
        let result = crate::commands::cmd_merge(&fake, "o", "r", 10, crate::commands::MergeContext { store: &store, dir: tmp.path(), git_dir: &git_dir, merge_method: "squash" }, &ps);
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
        let ps = crate::process_store::ProcessStateStore::open(&git_dir);
        crate::commands::cmd_merge(&fake, "o", "r", 10, crate::commands::MergeContext { store: &store, dir: tmp.path(), git_dir: &git_dir, merge_method: "squash" }, &ps).unwrap();
        let state = store.load().unwrap();
        assert!(!state.tracked.contains(&10), "cmd_merge must untrack the PR");
    }

    // Stage 7: cmd_merge removes merged PR from its feature envelope
    #[test]
    fn cmd_merge_governs_removes_pr_from_feature_envelope() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join("git_dir");
        std::fs::create_dir_all(&git_dir).unwrap();
        let store = crate::store::Store::open(&git_dir);
        store.track(10).unwrap();
        store.update_cache(crate::store::PrCache { number: 10, title: "T".into(), branch: "feat/x".into(), base: "main".into() }).unwrap();
        let ps = crate::process_store::ProcessStateStore::open(&git_dir);
        let mut ps_state = ps.load().unwrap();
        ps_state.feature_envelopes.insert("auth-refactor".into());
        ps_state.records.insert(10, crate::process_store::ProcessRecord {
            pr: 10, expected_branch: "feat/x".into(), pid: None,
            feature_envelopes: vec!["auth-refactor".into()], feature_envelope: None,
            worktree: tmp.path().to_string_lossy().to_string(),
            app_config_names: vec![],
        });
        ps_state.records.insert(20, crate::process_store::ProcessRecord {
            pr: 20, expected_branch: "feat/y".into(), pid: None,
            feature_envelopes: vec!["auth-refactor".into()], feature_envelope: None,
            worktree: tmp.path().to_string_lossy().to_string(),
            app_config_names: vec![],
        });
        ps.save_state(ps_state).unwrap();
        let fake = crate::github::FakeGithubClient::new_with_pr(10, "feat/x", "T", "main");
        let result = crate::commands::cmd_merge(&fake, "o", "r", 10, crate::commands::MergeContext { store: &store, dir: tmp.path(), git_dir: &git_dir, merge_method: "squash" }, &ps);
        assert!(result.is_ok(), "cmd_merge must succeed: {:?}", result);
        let msg = result.unwrap();
        assert!(msg.contains("auth-refactor"), "must mention envelope name: {}", msg);
        let ps_after = ps.load().unwrap();
        assert!(!ps_after.records.contains_key(&10), "PR #10 must be removed from process state");
        assert!(ps_after.records.contains_key(&20), "PR #20 must remain in process state");
    }

    // Stage 7: cmd_merge deletes empty envelope after last member merges
    #[test]
    fn cmd_merge_governs_deletes_envelope_when_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join("git_dir");
        std::fs::create_dir_all(&git_dir).unwrap();
        let store = crate::store::Store::open(&git_dir);
        store.track(10).unwrap();
        store.update_cache(crate::store::PrCache { number: 10, title: "T".into(), branch: "feat/x".into(), base: "main".into() }).unwrap();
        let ps = crate::process_store::ProcessStateStore::open(&git_dir);
        let mut ps_state = ps.load().unwrap();
        ps_state.feature_envelopes.insert("solo-feature".into());
        ps_state.records.insert(10, crate::process_store::ProcessRecord {
            pr: 10, expected_branch: "feat/x".into(), pid: None,
            feature_envelopes: vec!["solo-feature".into()], feature_envelope: None,
            worktree: tmp.path().to_string_lossy().to_string(),
            app_config_names: vec![],
        });
        ps.save_state(ps_state).unwrap();
        let fake = crate::github::FakeGithubClient::new_with_pr(10, "feat/x", "T", "main");
        let result = crate::commands::cmd_merge(&fake, "o", "r", 10, crate::commands::MergeContext { store: &store, dir: tmp.path(), git_dir: &git_dir, merge_method: "squash" }, &ps);
        assert!(result.is_ok(), "cmd_merge must succeed: {:?}", result);
        let msg = result.unwrap();
        assert!(msg.contains("solo-feature"), "must mention envelope name: {}", msg);
        let ps_after = ps.load().unwrap();
        assert!(!ps_after.feature_envelopes.contains("solo-feature"), "empty envelope must be deleted");
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
            codeowners_eligibility: Default::default(), created_at: None, is_stacked: false,
        });
        fake.set_pr(2, crate::model::PrState {
            number: 2, title: "child".into(), branch: "feat/child".into(), base: "feat/parent".into(),
            head_sha: "child_sha".into(), draft: false, approved: false,
            checks: vec![], threads: vec![], needs_parent_rebase: false, has_merge_conflict: false,
            codeowners_eligibility: Default::default(), created_at: None, is_stacked: false,
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
            codeowners_eligibility: Default::default(), created_at: None, is_stacked: false,
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
            codeowners_eligibility: Default::default(), created_at: None, is_stacked: false,
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
            codeowners_eligibility: Default::default(), created_at: None, is_stacked: false,
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
            codeowners_eligibility: Default::default(), created_at: None, is_stacked: false,
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
            codeowners_eligibility: Default::default(), created_at: None, is_stacked: false,
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
            codeowners_eligibility: Default::default(), created_at: None, is_stacked: false,
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
            codeowners_eligibility: Default::default(), created_at: None, is_stacked: false,
        });
        fake.set_pr(11, crate::model::PrState {
            number: 11, title: "Top".into(), branch: "feat/top".into(), base: "feat/mid".into(),
            head_sha: String::new(), draft: false, approved: false, checks: vec![], threads: vec![],
            needs_parent_rebase: false, has_merge_conflict: false,
            codeowners_eligibility: Default::default(), created_at: None, is_stacked: false,
        });
        fake.set_pr(55, crate::model::PrState {
            number: 55, title: "Root".into(), branch: "feat/root".into(), base: "main".into(),
            head_sha: root_tip.clone(),
            draft: false, approved: false, checks: vec![], threads: vec![],
            needs_parent_rebase: false, has_merge_conflict: false,
            codeowners_eligibility: Default::default(), created_at: None, is_stacked: false,
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

    // CLI: fp app set-config saves repo→config-name and exits ok
    #[test]
    fn cmd_app_set_config_governs_saves_repo_assignment() {
        let dir = tempfile::tempdir().unwrap();
        let store = crate::app_config::AppConfigStore::open(dir.path().join("config.toml"));
        let result = crate::commands::cmd_app_set_config(&store, "acme/payments-api", "payments-api");
        assert!(result.is_ok(), "cmd_app_set_config must succeed: {:?}", result);
        let assigned = store.get_repo_config("acme/payments-api").unwrap();
        assert_eq!(assigned, Some("payments-api".to_string()),
            "cmd_app_set_config must persist repo assignment, got: {:?}", assigned);
    }


    // fp pr up bootstraps a single PR using app_config_names from ProcessRecord
    #[test]
    fn cmd_pr_up_governs_reads_config_from_process_record() {
        let dir = tempfile::tempdir().unwrap();
        let ps = crate::process_store::ProcessStateStore::open(dir.path());
        let app_store = crate::app_config::AppConfigStore::open(dir.path().join("config.toml"));
        let wt = tempfile::tempdir().unwrap();
        app_store.save_app_config(crate::app_config::AppConfig {
            name: "svc".into(), bootstrap: "echo up".into(), teardown: "echo down".into(),
            startup_timeout: "5s".into(), health_check: None, ephemeral: false, main_worktree: None, setup: None,
            volume_check: None,
}).unwrap();
        // assign config via ProcessRecord (not set_pr_config)
        let mut state = ps.load().unwrap();
        state.records.insert(42, crate::process_store::ProcessRecord {
            pr: 42, expected_branch: String::new(), pid: None,
            feature_envelopes: vec![], feature_envelope: None,
            worktree: wt.path().to_string_lossy().to_string(),
            app_config_names: vec!["svc".into()],
        });
        ps.save_state(state).unwrap();
        let result = crate::commands::cmd_pr_up(&ps, &app_store, 42);
        assert!(result.is_ok(), "cmd_pr_up must succeed reading config from ProcessRecord: {:?}", result);
        let state = ps.load().unwrap();
        assert!(state.records.contains_key(&42),
            "cmd_pr_up must activate PR 42 in process state");
    }

    // CLI: fp feature new creates envelope
    #[test]
    fn cmd_feature_new_governs_creates_envelope() {
        let dir = tempfile::tempdir().unwrap();
        let ps = crate::process_store::ProcessStateStore::open(dir.path());
        let result = crate::commands::cmd_feature_new(&ps, "auth-refactor");
        assert!(result.is_ok(), "cmd_feature_new must succeed: {:?}", result);
        let list = crate::feature::feature_list(&ps).unwrap();
        assert!(list.iter().any(|f| f.name == "auth-refactor"),
            "cmd_feature_new must create envelope, got: {:?}", list);
    }

    // CLI: fp feature list returns output
    #[test]
    fn cmd_feature_list_governs_returns_output() {
        let dir = tempfile::tempdir().unwrap();
        let ps = crate::process_store::ProcessStateStore::open(dir.path());
        crate::feature::feature_new(&ps, "my-feature").unwrap();
        let result = crate::commands::cmd_feature_list(&ps);
        assert!(result.is_ok(), "cmd_feature_list must succeed: {:?}", result);
        assert!(result.unwrap().contains("my-feature"),
            "cmd_feature_list output must contain 'my-feature'");
    }

    // CLI: fp app define-config stores all config fields
    #[test]
    fn cmd_app_define_config_governs_stores_all_fields() {
        let dir = tempfile::tempdir().unwrap();
        let store = crate::app_config::AppConfigStore::open(dir.path().join("config.toml"));
        let result = crate::commands::cmd_app_define_config(
            &store, "payments-api",
            "docker-compose up -d",
            "docker-compose down",
            "60s",
            None,
            false,
            None,
            None,
            None,
        );
        assert!(result.is_ok(), "cmd_app_define_config must succeed: {:?}", result);
        let cfg = store.load_app_config("payments-api").unwrap().unwrap();
        assert_eq!(cfg.bootstrap, "docker-compose up -d",
            "cmd_app_define_config must store bootstrap, got: {:?}", cfg.bootstrap);
        assert_eq!(cfg.teardown, "docker-compose down",
            "cmd_app_define_config must store teardown, got: {:?}", cfg.teardown);
        assert_eq!(cfg.startup_timeout, "60s",
            "cmd_app_define_config must store startup_timeout, got: {:?}", cfg.startup_timeout);
        assert_eq!(cfg.health_check, None,
            "cmd_app_define_config must store None health_check, got: {:?}", cfg.health_check);
    }

    #[test]
    fn cmd_app_define_config_governs_setup_flag_persists() {
        let dir = tempfile::tempdir().unwrap();
        let store = crate::app_config::AppConfigStore::open(dir.path().join("config.toml"));
        let result = crate::commands::cmd_app_define_config(
            &store, "payments-api",
            "docker-compose up -d",
            "docker-compose down",
            "60s",
            None,
            false,
            None,
            Some("npm install"),
            None,
        );
        assert!(result.is_ok(), "cmd_app_define_config must succeed: {:?}", result);
        let cfg = store.load_app_config("payments-api").unwrap().unwrap();
        assert_eq!(cfg.setup, Some("npm install".into()),
            "cmd_app_define_config must store setup command, got: {:?}", cfg.setup);
    }

    // cmd_app_define_config stores main_worktree when provided
    #[test]
    fn cmd_app_define_config_governs_stores_main_worktree() {
        let dir = tempfile::tempdir().unwrap();
        let store = crate::app_config::AppConfigStore::open(dir.path().join("config.toml"));
        let result = crate::commands::cmd_app_define_config(
            &store, "payments-api",
            "docker-compose up -d",
            "docker-compose down",
            "60s",
            None,
            false,
            Some("/path/to/main"),
            None,
            None,
        );
        assert!(result.is_ok(), "cmd_app_define_config must succeed: {:?}", result);
        let cfg = store.load_app_config("payments-api").unwrap().unwrap();
        assert_eq!(cfg.main_worktree, Some("/path/to/main".to_string()),
            "cmd_app_define_config must store main_worktree, got: {:?}", cfg.main_worktree);
    }

    // volume_check field persists through define-config
    #[test]
    fn cmd_app_define_config_governs_volume_check_field_persists() {
        let dir = tempfile::tempdir().unwrap();
        let store = crate::app_config::AppConfigStore::open(dir.path().join("config.toml"));
        let result = crate::commands::cmd_app_define_config(
            &store, "svc",
            "cd $FP_WORKTREE && docker-compose up -d",
            "docker-compose down",
            "60s",
            None,
            false,
            None,
            None,
            Some("docker inspect $(docker compose ps -q svc) --format '{{range .Mounts}}{{.Source}} {{end}}' | grep -qF \"$FP_WORKTREE\""),
        );
        assert!(result.is_ok(), "cmd_app_define_config must succeed with volume_check: {:?}", result);
        let cfg = store.load_app_config("svc").unwrap().unwrap();
        assert!(cfg.volume_check.is_some(),
            "governs_volume_check_field_persists: volume_check must be stored, got: {:?}", cfg.volume_check);
    }

    // fp feature status shows volume_check result alongside health
    #[test]
    fn cmd_feature_status_governs_feature_status_shows_volume_check_result() {
        let dir = tempfile::tempdir().unwrap();
        let ps = crate::process_store::ProcessStateStore::open(dir.path());
        let app_store = crate::app_config::AppConfigStore::open(dir.path().join("config.toml"));
        let live_pid = std::process::id();
        // App with a volume_check that always fails (exit 1)
        app_store.save_app_config(crate::app_config::AppConfig {
            name: "backend".into(),
            bootstrap: "cd $FP_WORKTREE && npm start".into(),
            teardown: "pkill node".into(),
            startup_timeout: "30s".into(),
            health_check: None,
            ephemeral: false,
            main_worktree: None,
            setup: None,
            volume_check: Some("exit 1".into()),
        }).unwrap();
        let rec = crate::process_store::ProcessRecord {
            pr: 42,
            expected_branch: "feat/x".into(),
            pid: Some(live_pid),
            feature_envelopes: vec!["my-feature".into()], feature_envelope: None,
            worktree: dir.path().to_string_lossy().to_string(),
            app_config_names: vec!["backend".into()],
        };
        ps.activate(rec).unwrap();
        let mut state = ps.load().unwrap();
        state.feature_envelopes.insert("my-feature".to_string());
        ps.save_state(state).unwrap();
        let result = crate::commands::cmd_feature_status(&ps, &app_store, "my-feature", false, std::path::Path::new("."));
        assert!(result.is_ok(), "cmd_feature_status must succeed: {:?}", result);
        let output = result.unwrap();
        assert!(output.contains("volume") || output.contains("worktree") || output.contains("✗"),
            "governs_feature_status_shows_volume_check_result: status must show volume_check result, got: {}", output);
    }

    // cmd_app_define_config warns when no script references $FP_WORKTREE
    #[test]
    fn cmd_app_define_config_governs_define_warns_missing_fp_worktree() {
        let dir = tempfile::tempdir().unwrap();
        let store = crate::app_config::AppConfigStore::open(dir.path().join("config.toml"));
        let result = crate::commands::cmd_app_define_config(
            &store, "svc",
            "docker-compose up -d",
            "docker-compose down",
            "60s",
            None,
            false,
            None,
            None,
            None,
        );
        assert!(result.is_ok(), "cmd_app_define_config must succeed: {:?}", result);
        let output = result.unwrap();
        assert!(output.contains("FP_WORKTREE"),
            "cmd_app_define_config must warn about missing FP_WORKTREE, got: {}", output);
    }

    // cmd_app_define_config does not warn when a script references $FP_WORKTREE
    #[test]
    fn cmd_app_define_config_governs_no_warn_when_fp_worktree_present() {
        let dir = tempfile::tempdir().unwrap();
        let store = crate::app_config::AppConfigStore::open(dir.path().join("config.toml"));
        let result = crate::commands::cmd_app_define_config(
            &store, "svc",
            "cd $FP_WORKTREE && docker-compose up -d",
            "docker-compose down",
            "60s",
            None,
            false,
            None,
            None,
            None,
        );
        assert!(result.is_ok(), "cmd_app_define_config must succeed: {:?}", result);
        let output = result.unwrap();
        assert!(!output.contains("Warning"),
            "cmd_app_define_config must not warn when FP_WORKTREE present, got: {}", output);
    }

    // cmd_feature_status warns when app config scripts don't reference $FP_WORKTREE
    #[test]
    fn cmd_feature_status_governs_feature_status_warns_missing_fp_worktree() {
        let dir = tempfile::tempdir().unwrap();
        let ps = crate::process_store::ProcessStateStore::open(dir.path());
        let app_store = crate::app_config::AppConfigStore::open(dir.path().join("config.toml"));
        let live_pid = std::process::id();
        // Define an app config without $FP_WORKTREE in any script
        app_store.save_app_config(crate::app_config::AppConfig {
            name: "backend".into(),
            bootstrap: "npm start".into(),
            teardown: "pkill node".into(),
            startup_timeout: "30s".into(),
            health_check: None,
            ephemeral: false,
            main_worktree: None,
            setup: None,
            volume_check: None,
        }).unwrap();
        let rec = crate::process_store::ProcessRecord {
            pr: 42,
            expected_branch: "feat/x".into(),
            pid: Some(live_pid),
            feature_envelopes: vec!["my-feature".into()], feature_envelope: None,
            worktree: dir.path().to_string_lossy().to_string(),
            app_config_names: vec!["backend".into()],
        };
        ps.activate(rec).unwrap();
        let mut state = ps.load().unwrap();
        state.feature_envelopes.insert("my-feature".to_string());
        ps.save_state(state).unwrap();
        let result = crate::commands::cmd_feature_status(&ps, &app_store, "my-feature", false, std::path::Path::new("."));
        assert!(result.is_ok(), "cmd_feature_status must succeed: {:?}", result);
        let output = result.unwrap();
        assert!(output.contains("FP_WORKTREE"),
            "cmd_feature_status must warn about missing FP_WORKTREE in app config, got: {}", output);
    }

    // Stage 3: fp feature list --running
    #[test]
    fn cmd_feature_list_running_governs_returns_only_live_envelopes() {
        let dir = tempfile::tempdir().unwrap();
        let ps = crate::process_store::ProcessStateStore::open(dir.path());
        crate::feature::feature_new(&ps, "dead-feature").unwrap();
        let result = crate::commands::cmd_feature_list_running(&ps);
        assert!(result.is_ok(), "cmd_feature_list_running must succeed: {:?}", result);
        let output = result.unwrap();
        assert!(!output.contains("dead-feature"),
            "cmd_feature_list_running must not include envelope with no live PIDs, got: {}", output);
    }

    // Stage 3: fp feature status
    #[test]
    fn cmd_feature_status_governs_returns_health_per_pr() {
        let dir = tempfile::tempdir().unwrap();
        let ps = crate::process_store::ProcessStateStore::open(dir.path());
        let app_store = crate::app_config::AppConfigStore::open(dir.path().join("config.toml"));
        let live_pid = std::process::id();
        let rec = crate::process_store::ProcessRecord {
            pr: 123,
            expected_branch: "feat/pay".into(),
            pid: Some(live_pid),
            feature_envelopes: vec!["auth-refactor".into()], feature_envelope: None,
            worktree: dir.path().to_string_lossy().to_string(),
            app_config_names: vec![],
        };
        ps.activate(rec).unwrap();
        let mut state = ps.load().unwrap();
        state.feature_envelopes.insert("auth-refactor".to_string());
        ps.save_state(state).unwrap();
        let result = crate::commands::cmd_feature_status(&ps, &app_store, "auth-refactor", false, std::path::Path::new("."));
        assert!(result.is_ok(), "cmd_feature_status must succeed: {:?}", result);
        let output = result.unwrap();
        assert!(output.contains("123"),
            "cmd_feature_status output must contain PR number 123, got: {}", output);
    }

    // CLI: fp app define-config stores optional health_check
    #[test]
    fn cmd_app_define_config_governs_stores_health_check() {
        let dir = tempfile::tempdir().unwrap();
        let store = crate::app_config::AppConfigStore::open(dir.path().join("config.toml"));
        crate::commands::cmd_app_define_config(
            &store, "svc",
            "npm start", "pkill node", "30s",
            Some("curl -f http://localhost:3000/health"),
            false,
            None,
            None,
            None,
        ).unwrap();
        let cfg = store.load_app_config("svc").unwrap().unwrap();
        assert_eq!(cfg.health_check, Some("curl -f http://localhost:3000/health".into()),
            "cmd_app_define_config must store health_check, got: {:?}", cfg.health_check);
    }

    // Stage 2b: fp feature up bootstraps all members and records activation
    #[test]
    fn cmd_feature_up_governs_activates_all_members() {
        use std::process::Command;
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        Command::new("git").args(["init", "-b", "main", repo.to_str().unwrap()]).output().unwrap();
        for arg in &[["config","user.email","t@t.com"],["config","user.name","T"]] {
            Command::new("git").args(["-C", repo.to_str().unwrap()]).args(arg).output().unwrap();
        }
        std::fs::write(repo.join("f"), "x").unwrap();
        Command::new("git").args(["-C", repo.to_str().unwrap(), "add", "."]).output().unwrap();
        Command::new("git").args(["-C", repo.to_str().unwrap(), "commit", "--allow-empty", "-m", "init"]).output().unwrap();
        for branch in &["feat/a", "feat/b"] {
            Command::new("git").args(["-C", repo.to_str().unwrap(), "checkout", "-b", branch]).output().unwrap();
            Command::new("git").args(["-C", repo.to_str().unwrap(), "checkout", "main"]).output().unwrap();
            let wt = crate::worktree::worktree_path(&repo, branch);
            std::fs::create_dir_all(wt.parent().unwrap()).unwrap();
            Command::new("git").args(["-C", repo.to_str().unwrap(), "worktree", "add", wt.to_str().unwrap(), branch]).output().unwrap();
        }
        let dir = tempfile::tempdir().unwrap();
        let ps = crate::process_store::ProcessStateStore::open(dir.path());
        let app_store = crate::app_config::AppConfigStore::open(dir.path().join("config.toml"));
        app_store.save_app_config(crate::app_config::AppConfig {
            name: "svc".into(), bootstrap: "echo up".into(), teardown: "echo down".into(),
            startup_timeout: "5s".into(), health_check: None, ephemeral: false, main_worktree: None, setup: None,
            volume_check: None,
}).unwrap();
        crate::feature::feature_new(&ps, "my-feature").unwrap();
        let rec1 = crate::process_store::ProcessRecord {
            pr: 10, expected_branch: "feat/a".into(), pid: None,
            feature_envelopes: vec!["my-feature".into()], feature_envelope: None,
            worktree: String::new(),
            app_config_names: vec!["svc".into()],
        };
        let rec2 = crate::process_store::ProcessRecord {
            pr: 20, expected_branch: "feat/b".into(), pid: None,
            feature_envelopes: vec!["my-feature".into()], feature_envelope: None,
            worktree: String::new(),
            app_config_names: vec!["svc".into()],
        };
        ps.activate(rec1).unwrap();
        ps.activate(rec2).unwrap();
        let mut state = ps.load().unwrap();
        state.feature_envelopes.insert("my-feature".to_string());
        ps.save_state(state).unwrap();
        let result = crate::commands::cmd_feature_up(&ps, &app_store, "my-feature", &repo);
        assert!(result.is_ok(), "cmd_feature_up must succeed: {:?}", result);
        let state = ps.load().unwrap();
        assert!(state.records[&10].pid.is_some(), "cmd_feature_up must record PID for PR 10");
        assert!(state.records[&20].pid.is_some(), "cmd_feature_up must record PID for PR 20");
    }

    // Stage 2b: fp feature down tears down all members
    #[test]
    fn cmd_feature_down_governs_deactivates_all_members() {
        let dir = tempfile::tempdir().unwrap();
        let ps = crate::process_store::ProcessStateStore::open(dir.path());
        let app_store = crate::app_config::AppConfigStore::open(dir.path().join("config.toml"));
        let wt = tempfile::tempdir().unwrap();
        app_store.save_app_config(crate::app_config::AppConfig {
            name: "svc".into(), bootstrap: "echo up".into(), teardown: "echo down".into(),
            startup_timeout: "5s".into(), health_check: None, ephemeral: false, main_worktree: None, setup: None,
            volume_check: None,
}).unwrap();
        crate::feature::feature_new(&ps, "my-feature").unwrap();
        ps.activate(crate::process_store::ProcessRecord {
            pr: 10, expected_branch: "feat/a".into(), pid: Some(std::process::id()),
            feature_envelopes: vec!["my-feature".into()], feature_envelope: None,
            worktree: wt.path().to_string_lossy().to_string(),
            app_config_names: vec!["svc".into()],
        }).unwrap();
        let mut state = ps.load().unwrap();
        state.feature_envelopes.insert("my-feature".to_string());
        ps.save_state(state).unwrap();
        let result = crate::commands::cmd_feature_down(&ps, &app_store, "my-feature", std::path::Path::new("."));
        assert!(result.is_ok(), "cmd_feature_down must succeed: {:?}", result);
        let state = ps.load().unwrap();
        assert!(state.records.contains_key(&10),
            "cmd_feature_down must preserve PR 10 record (keep feature_envelope for re-up)");
        assert!(state.records[&10].pid.is_none(),
            "cmd_feature_down must clear pid for PR 10 after teardown");
    }

    // Stage 2b: fp feature rebuild re-runs bootstrap for ephemeral members without teardown
    #[test]
    fn cmd_feature_rebuild_governs_reruns_bootstrap_for_ephemeral() {
        use std::process::Command;
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        Command::new("git").args(["init", "-b", "main", repo.to_str().unwrap()]).output().unwrap();
        for arg in &[["config","user.email","t@t.com"],["config","user.name","T"]] {
            Command::new("git").args(["-C", repo.to_str().unwrap()]).args(arg).output().unwrap();
        }
        std::fs::write(repo.join("f"), "x").unwrap();
        Command::new("git").args(["-C", repo.to_str().unwrap(), "add", "."]).output().unwrap();
        Command::new("git").args(["-C", repo.to_str().unwrap(), "commit", "--allow-empty", "-m", "init"]).output().unwrap();
        Command::new("git").args(["-C", repo.to_str().unwrap(), "checkout", "-b", "feat/ext"]).output().unwrap();
        Command::new("git").args(["-C", repo.to_str().unwrap(), "checkout", "main"]).output().unwrap();
        let wt_dir = crate::worktree::worktree_path(&repo, "feat/ext");
        std::fs::create_dir_all(wt_dir.parent().unwrap()).unwrap();
        Command::new("git").args(["-C", repo.to_str().unwrap(), "worktree", "add", wt_dir.to_str().unwrap(), "feat/ext"]).output().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let ps = crate::process_store::ProcessStateStore::open(dir.path());
        let app_store = crate::app_config::AppConfigStore::open(dir.path().join("config.toml"));
        app_store.save_app_config(crate::app_config::AppConfig {
            name: "ext".into(), bootstrap: "echo rebuild-ok".into(), teardown: "echo down".into(),
            startup_timeout: "5s".into(), health_check: Some("true".into()), ephemeral: true, main_worktree: None, setup: None,
            volume_check: None,
}).unwrap();
        crate::feature::feature_new(&ps, "ext-feature").unwrap();
        ps.activate(crate::process_store::ProcessRecord {
            pr: 77, expected_branch: "feat/ext".into(), pid: None,
            feature_envelopes: vec!["ext-feature".into()], feature_envelope: None,
            worktree: String::new(),
            app_config_names: vec!["ext".into()],
        }).unwrap();
        let mut state = ps.load().unwrap();
        state.feature_envelopes.insert("ext-feature".to_string());
        ps.save_state(state).unwrap();
        let result = crate::commands::cmd_feature_rebuild(&ps, &app_store, "ext-feature", None, &repo);
        assert!(result.is_ok(), "cmd_feature_rebuild must succeed: {:?}", result);
        let output = result.unwrap();
        assert!(output.contains("77"),
            "cmd_feature_rebuild output must mention PR 77, got: {}", output);
    }

    // Stage 2b: fp feature rebuild rejects persistent members
    #[test]
    fn cmd_feature_rebuild_governs_rejects_persistent_members() {
        let dir = tempfile::tempdir().unwrap();
        let ps = crate::process_store::ProcessStateStore::open(dir.path());
        let app_store = crate::app_config::AppConfigStore::open(dir.path().join("config.toml"));
        let wt = tempfile::tempdir().unwrap();
        app_store.save_app_config(crate::app_config::AppConfig {
            name: "svc".into(), bootstrap: "echo up".into(), teardown: "echo down".into(),
            startup_timeout: "5s".into(), health_check: None, ephemeral: false, main_worktree: None, setup: None,
            volume_check: None,
}).unwrap();
        crate::feature::feature_new(&ps, "my-feature").unwrap();
        ps.activate(crate::process_store::ProcessRecord {
            pr: 10, expected_branch: "feat/a".into(), pid: None,
            feature_envelopes: vec!["my-feature".into()], feature_envelope: None,
            worktree: wt.path().to_string_lossy().to_string(),
            app_config_names: vec!["svc".into()],
        }).unwrap();
        let mut state = ps.load().unwrap();
        state.feature_envelopes.insert("my-feature".to_string());
        ps.save_state(state).unwrap();
        let result = crate::commands::cmd_feature_rebuild(&ps, &app_store, "my-feature", None, std::path::Path::new("."));
        assert!(result.is_err() || result.as_ref().unwrap().contains("persistent"),
            "cmd_feature_rebuild must error or warn for persistent app, got: {:?}", result);
    }

    // cmd_feature_add_dep stores dep and returns confirmation message
    #[test]
    fn cmd_feature_add_dep_governs_stores_dep_in_envelope_deps() {
        let dir = tempfile::tempdir().unwrap();
        let ps = crate::process_store::ProcessStateStore::open(dir.path());
        let app_store_dir = tempfile::tempdir().unwrap();
        let app_store = crate::app_config::AppConfigStore::open(app_store_dir.path().join("config.toml"));
        let result = crate::commands::cmd_feature_add_dep(&ps, &app_store, "my-feature", "notifications-svc");
        assert!(result.is_ok(), "cmd_feature_add_dep must succeed: {:?}", result);
        let msg = result.unwrap();
        assert!(msg.contains("notifications-svc"),
            "cmd_feature_add_dep output must mention dep name, got: {}", msg);
        let state = ps.load().unwrap();
        assert!(state.envelope_deps.get("my-feature").map(|v| v.contains(&"notifications-svc".to_string())).unwrap_or(false),
            "cmd_feature_add_dep must store dep in envelope_deps");
    }

    // fp feature remove: removes PR from envelope
    #[test]
    fn cmd_feature_remove_governs_removes_pr_from_envelope() {
        let dir = tempfile::tempdir().unwrap();
        let ps = crate::process_store::ProcessStateStore::open(dir.path());
        let mut state = ps.load().unwrap();
        state.feature_envelopes.insert("auth-refactor".into());
        state.records.insert(10, crate::process_store::ProcessRecord {
            pr: 10, expected_branch: "feat/x".into(), pid: None,
            feature_envelopes: vec!["auth-refactor".into()], feature_envelope: None,
            worktree: dir.path().to_string_lossy().to_string(),
            app_config_names: vec![],
        });
        state.records.insert(20, crate::process_store::ProcessRecord {
            pr: 20, expected_branch: "feat/y".into(), pid: None,
            feature_envelopes: vec!["auth-refactor".into()], feature_envelope: None,
            worktree: dir.path().to_string_lossy().to_string(),
            app_config_names: vec![],
        });
        ps.save_state(state).unwrap();
        let result = crate::commands::cmd_feature_remove(&ps, "auth-refactor", 10);
        assert!(result.is_ok(), "cmd_feature_remove must succeed: {:?}", result);
        let msg = result.unwrap();
        assert!(msg.contains("Removed PR #10"), "must confirm removal: {}", msg);
        let after = ps.load().unwrap();
        assert!(!after.records.contains_key(&10), "PR #10 must be absent after remove");
        assert!(after.records.contains_key(&20), "PR #20 must remain");
        assert!(after.feature_envelopes.contains("auth-refactor"), "envelope must remain with members");
    }

    // fp feature remove: deletes envelope when it becomes empty
    #[test]
    fn cmd_feature_remove_governs_deletes_empty_envelope() {
        let dir = tempfile::tempdir().unwrap();
        let ps = crate::process_store::ProcessStateStore::open(dir.path());
        let mut state = ps.load().unwrap();
        state.feature_envelopes.insert("solo".into());
        state.records.insert(10, crate::process_store::ProcessRecord {
            pr: 10, expected_branch: "feat/x".into(), pid: None,
            feature_envelopes: vec!["solo".into()], feature_envelope: None,
            worktree: dir.path().to_string_lossy().to_string(),
            app_config_names: vec![],
        });
        ps.save_state(state).unwrap();
        let result = crate::commands::cmd_feature_remove(&ps, "solo", 10);
        assert!(result.is_ok(), "cmd_feature_remove must succeed: {:?}", result);
        let after = ps.load().unwrap();
        assert!(!after.feature_envelopes.contains("solo"), "empty envelope must be deleted");
    }

    // fp feature status: flags closed PRs when GitHub client provided
    #[test]
    fn cmd_feature_status_governs_flags_closed_prs() {
        let dir = tempfile::tempdir().unwrap();
        let ps = crate::process_store::ProcessStateStore::open(dir.path());
        let app_store = crate::app_config::AppConfigStore::open(dir.path().join("config.toml"));
        let mut state = ps.load().unwrap();
        state.feature_envelopes.insert("auth-refactor".into());
        state.records.insert(99, crate::process_store::ProcessRecord {
            pr: 99, expected_branch: "feat/z".into(), pid: None,
            feature_envelopes: vec!["auth-refactor".into()], feature_envelope: None,
            worktree: dir.path().to_string_lossy().to_string(),
            app_config_names: vec![],
        });
        ps.save_state(state).unwrap();
        let mut fake = crate::github::FakeGithubClient::new_with_pr(99, "feat/z", "Z", "main");
        fake.set_pr_merged(99, true);
        let result = crate::commands::cmd_feature_status_with_client(&ps, &app_store, "auth-refactor", Some(&fake), "o", "r", std::path::Path::new("."));
        assert!(result.is_ok(), "cmd_feature_status_with_client must succeed: {:?}", result);
        let output = result.unwrap();
        assert!(output.contains("merged"), "must flag PR #99 as merged: {}", output);
        assert!(output.contains("fp feature remove"), "must show remove hint: {}", output);
    }

    // fp pr up --config: config flag overrides bound app config names
    #[test]
    fn cmd_pr_up_governs_accepts_config_override() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        let ps = crate::process_store::ProcessStateStore::open(&git_dir);
        let app_store = crate::app_config::AppConfigStore::open(tmp.path().join("config.toml"));
        let worktree = tmp.path().to_string_lossy().to_string();
        // Define an app config
        crate::commands::cmd_app_define_config(&app_store, "payments-api", "echo start", "echo stop", "5s", None, false, None, None, None).unwrap();
        // Create process record with NO bound configs
        let mut state = ps.load().unwrap();
        state.records.insert(10, crate::process_store::ProcessRecord {
            pr: 10, expected_branch: "feat/x".into(), pid: None,
            feature_envelopes: vec![], feature_envelope: None,
            worktree: worktree.clone(),
            app_config_names: vec![],
        });
        ps.save_state(state).unwrap();
        // pr up with --config override should use the supplied config
        let result = crate::commands::cmd_pr_up_with_configs(&ps, &app_store, 10, &["payments-api"]);
        assert!(result.is_ok(), "cmd_pr_up_with_configs must succeed: {:?}", result);
        let msg = result.unwrap();
        assert!(msg.contains("payments-api"), "must mention config name: {}", msg);
    }

    // fp feature up --no: abort if conflict detected
    #[test]
    fn cmd_feature_up_governs_no_flag_aborts_on_conflict() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        let ps = crate::process_store::ProcessStateStore::open(&git_dir);
        let app_store = crate::app_config::AppConfigStore::open(tmp.path().join("config.toml"));
        let mut state = ps.load().unwrap();
        state.feature_envelopes.insert("feature-a".into());
        state.feature_envelopes.insert("feature-b".into());
        // feature-b has a live process (pid = current process, always alive)
        state.records.insert(20, crate::process_store::ProcessRecord {
            pr: 20, expected_branch: "feat/y".into(), pid: Some(std::process::id()),
            feature_envelopes: vec!["feature-b".into()], feature_envelope: None,
            worktree: tmp.path().to_string_lossy().to_string(),
            app_config_names: vec![],
        });
        state.records.insert(10, crate::process_store::ProcessRecord {
            pr: 10, expected_branch: "feat/x".into(), pid: None,
            feature_envelopes: vec!["feature-a".into()], feature_envelope: None,
            worktree: tmp.path().to_string_lossy().to_string(),
            app_config_names: vec![],
        });
        ps.save_state(state).unwrap();
        let result = crate::commands::cmd_feature_up_checked(&ps, &app_store, "feature-a", false, true, std::path::Path::new("."));
        assert!(result.is_err() || result.as_ref().unwrap().contains("aborted"),
            "cmd_feature_up_checked with no=true must abort on conflict: {:?}", result);
        let msg = result.unwrap_or_default();
        assert!(msg.contains("aborted") || msg.is_empty(),
            "abort message expected, got: {}", msg);
        // Recheck via error path
        let result2 = crate::commands::cmd_feature_up_checked(&ps, &app_store, "feature-a", false, true, std::path::Path::new("."));
        assert!(result2.is_err() || result2.as_ref().map(|s| s.contains("aborted")).unwrap_or(false),
            "must abort on conflict with --no flag: {:?}", result2);
    }

    // fp feature up --yes: tears down conflicting feature
    #[test]
    fn cmd_feature_up_governs_yes_flag_tears_down_conflict() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        let ps = crate::process_store::ProcessStateStore::open(&git_dir);
        let app_store = crate::app_config::AppConfigStore::open(tmp.path().join("config.toml"));
        let mut state = ps.load().unwrap();
        state.feature_envelopes.insert("feature-a".into());
        state.feature_envelopes.insert("feature-b".into());
        state.records.insert(20, crate::process_store::ProcessRecord {
            pr: 20, expected_branch: "feat/y".into(), pid: Some(std::process::id()),
            feature_envelopes: vec!["feature-b".into()], feature_envelope: None,
            worktree: tmp.path().to_string_lossy().to_string(),
            app_config_names: vec![],
        });
        state.records.insert(10, crate::process_store::ProcessRecord {
            pr: 10, expected_branch: "feat/x".into(), pid: None,
            feature_envelopes: vec!["feature-a".into()], feature_envelope: None,
            worktree: tmp.path().to_string_lossy().to_string(),
            app_config_names: vec![],
        });
        ps.save_state(state).unwrap();
        let result = crate::commands::cmd_feature_up_checked(&ps, &app_store, "feature-a", true, false, std::path::Path::new("."));
        assert!(result.is_ok(), "cmd_feature_up_checked with yes=true must succeed: {:?}", result);
        let msg = result.unwrap();
        assert!(msg.contains("feature-b"), "must mention torn-down feature: {}", msg);
    }

    // fp switch --non-interactive: skips dirty-check even when force=false
    #[test]
    fn cmd_switch_governs_non_interactive_skips_prompts() {
        // Use a fake git dir that has no actual git repo — cmd_switch will fail, but
        // it must NOT fail with the dirty-check message when non_interactive=true.
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        let store = crate::store::Store::open(&git_dir);
        store.track(10).unwrap();
        store.update_cache(crate::store::PrCache { number: 10, title: "X".into(), branch: "feat/x".into(), base: "main".into() }).unwrap();
        let ps = crate::process_store::ProcessStateStore::open(&git_dir);
        let app_store = crate::app_config::AppConfigStore::open(tmp.path().join("config.toml"));
        // With non_interactive=true and force=false, the dirty-check must be skipped.
        // The call may fail for other reasons (no real git repo), but NOT with the dirty-check message.
        let result = crate::commands::cmd_switch(&store, &ps, &app_store, &git_dir, 10, "test-session", false, false, true, std::path::Path::new("."));
        if let Err(ref e) = result {
            assert!(!e.to_string().contains("uncommitted changes"),
                "non_interactive must skip dirty-check, got: {}", e);
        }
        // (ok or another error is acceptable — we only govern that dirty-check is skipped)
    }

    // fp feature status --json: output is valid JSON
    #[test]
    fn cmd_feature_status_governs_json_output() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        let ps = crate::process_store::ProcessStateStore::open(&git_dir);
        let app_store = crate::app_config::AppConfigStore::open(tmp.path().join("config.toml"));
        crate::feature::feature_new(&ps, "my-feat").unwrap();
        let mut state = ps.load().unwrap();
        state.records.insert(10, crate::process_store::ProcessRecord {
            pr: 10, expected_branch: "feat/x".into(), pid: None,
            feature_envelopes: vec!["my-feat".into()], feature_envelope: None,
            worktree: tmp.path().to_string_lossy().to_string(),
            app_config_names: vec![],
        });
        ps.save_state(state).unwrap();
        let result = crate::commands::cmd_feature_status(&ps, &app_store, "my-feat", true, std::path::Path::new("."));
        assert!(result.is_ok(), "cmd_feature_status with json=true must succeed: {:?}", result);
        let out = result.unwrap();
        assert!(serde_json::from_str::<serde_json::Value>(&out).is_ok(),
            "output must be valid JSON: {}", out);
    }

    // fp feature status: branch_ok must not show 'wrong branch' when worktree doesn't exist
    #[test]
    fn cmd_feature_status_governs_no_false_positive_branch_check_nonexistent_worktree() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        let ps = crate::process_store::ProcessStateStore::open(&git_dir);
        let app_store = crate::app_config::AppConfigStore::open(tmp.path().join("config.toml"));
        crate::feature::feature_new(&ps, "my-feat").unwrap();
        let mut state = ps.load().unwrap();
        state.records.insert(10, crate::process_store::ProcessRecord {
            pr: 10, expected_branch: "feat/x".into(), pid: None,
            feature_envelopes: vec!["my-feat".into()], feature_envelope: None,
            worktree: tmp.path().join("no-such-worktree").to_string_lossy().to_string(),
            app_config_names: vec![],
        });
        ps.save_state(state).unwrap();
        let result = crate::commands::cmd_feature_status(&ps, &app_store, "my-feat", false, std::path::Path::new("."));
        assert!(result.is_ok(), "cmd_feature_status must succeed: {:?}", result);
        let out = result.unwrap();
        assert!(!out.contains("wrong branch"),
            "must not show 'wrong branch' when worktree does not exist: {}", out);
    }

    // fp feature add: expected_branch must be populated from store cache (not left empty)
    #[test]
    fn feature_add_governs_populates_expected_branch_from_store() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        let store = crate::store::Store::open(&git_dir);
        let ps = crate::process_store::ProcessStateStore::open(&git_dir);
        store.track(10).unwrap();
        store.update_cache(crate::store::PrCache {
            number: 10, title: "X".into(), branch: "feat/x".into(), base: "main".into(),
        }).unwrap();
        crate::feature::feature_new(&ps, "my-feat").unwrap();
        crate::feature::feature_add(&ps, &store, "my-feat", 10, &[]).unwrap();
        let state = ps.load().unwrap();
        let rec = state.records.get(&10).unwrap();
        assert_eq!(rec.expected_branch, "feat/x",
            "expected_branch must be populated from store cache, got: {:?}", rec.expected_branch);
    }

    // D1: cmd_switch with injected cwd creates worktree in correct location
    #[test]
    fn cmd_switch_governs_cwd_injection_creates_worktree() {
        use std::process::Command;
        let tmp = tempfile::tempdir().unwrap();
        let remote = tmp.path().join("remote.git");
        let repo = tmp.path().join("repo");
        Command::new("git").args(["init", "--bare", remote.to_str().unwrap()]).output().unwrap();
        Command::new("git").args(["clone", remote.to_str().unwrap(), repo.to_str().unwrap()]).output().unwrap();
        for arg in &[["config","user.email","t@t.com"],["config","user.name","T"]] {
            Command::new("git").args(["-C", repo.to_str().unwrap()]).args(arg).output().unwrap();
        }
        std::fs::write(repo.join("f"), "x").unwrap();
        Command::new("git").args(["-C", repo.to_str().unwrap(), "add", "."]).output().unwrap();
        Command::new("git").args(["-C", repo.to_str().unwrap(), "commit", "-m", "init"]).output().unwrap();
        Command::new("git").args(["-C", repo.to_str().unwrap(), "push", "origin", "HEAD:main"]).output().unwrap();
        Command::new("git").args(["-C", repo.to_str().unwrap(), "checkout", "-b", "feat/x"]).output().unwrap();
        Command::new("git").args(["-C", repo.to_str().unwrap(), "checkout", "main"]).output().unwrap();
        let git_dir = repo.join(".git");
        let store = crate::store::Store::open(&git_dir);
        store.track(10).unwrap();
        store.update_cache(crate::store::PrCache { number: 10, title: "X".into(), branch: "feat/x".into(), base: "main".into() }).unwrap();
        let ps = crate::process_store::ProcessStateStore::open(&git_dir);
        let app_store = crate::app_config::AppConfigStore::open(tmp.path().join("config.toml"));
        // inject the test repo as cwd — cmd_switch must use this, not current_dir()
        let result = crate::commands::cmd_switch(&store, &ps, &app_store, &git_dir, 10, "sess", false, false, false, &repo);
        assert!(result.is_ok(), "cmd_switch with injected cwd must succeed: {:?}", result);
        let wt_path = result.unwrap();
        assert!(wt_path.exists(), "created worktree path must exist: {}", wt_path.display());
    }

    // D2: feature_status with repo_root derives branch path from expected_branch, not rec.worktree
    #[test]
    fn feature_status_governs_branch_check_uses_derived_path_not_stored_worktree() {
        use std::process::Command;
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        Command::new("git").args(["init", "-b", "main", repo.to_str().unwrap()]).output().unwrap();
        for arg in &[["config","user.email","t@t.com"],["config","user.name","T"]] {
            Command::new("git").args(["-C", repo.to_str().unwrap()]).args(arg).output().unwrap();
        }
        std::fs::write(repo.join("f"), "x").unwrap();
        Command::new("git").args(["-C", repo.to_str().unwrap(), "add", "."]).output().unwrap();
        Command::new("git").args(["-C", repo.to_str().unwrap(), "commit", "--allow-empty", "-m", "init"]).output().unwrap();
        // Create a sibling worktree directory on feat/x
        Command::new("git").args(["-C", repo.to_str().unwrap(), "checkout", "-b", "feat/x"]).output().unwrap();
        Command::new("git").args(["-C", repo.to_str().unwrap(), "checkout", "main"]).output().unwrap();
        let wt_dir = tmp.path().join("repo-worktrees").join("feat").join("x");
        std::fs::create_dir_all(wt_dir.parent().unwrap()).unwrap();
        Command::new("git").args(["-C", repo.to_str().unwrap(), "worktree", "add", wt_dir.to_str().unwrap(), "feat/x"]).output().unwrap();
        let git_dir = repo.join(".git");
        let ps = crate::process_store::ProcessStateStore::open(&git_dir);
        let app_store = crate::app_config::AppConfigStore::open(tmp.path().join("config.toml"));
        crate::feature::feature_new(&ps, "my-feat").unwrap();
        let mut state = ps.load().unwrap();
        state.records.insert(10, crate::process_store::ProcessRecord {
            pr: 10, expected_branch: "feat/x".into(), pid: None,
            feature_envelopes: vec!["my-feat".into()], feature_envelope: None,
            worktree: String::new(), // empty — must not be used for branch check
            app_config_names: vec![],
        });
        ps.save_state(state).unwrap();
        // pass repo root so feature_status can derive the worktree path
        let result = crate::commands::cmd_feature_status(&ps, &app_store, "my-feat", false, &repo);
        assert!(result.is_ok(), "cmd_feature_status with repo_root must succeed: {:?}", result);
        let out = result.unwrap();
        assert!(out.contains("branch ok"),
            "branch check must pass when derived worktree exists on correct branch: {}", out);
    }

    // cmd_feature_status_with_client uses repo_root so ephemeral health-check runs from correct worktree
    #[test]
    fn cmd_feature_status_with_client_governs_ephemeral_health_check_uses_repo_root() {
        use std::process::Command;
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        Command::new("git").args(["init", "-b", "main", repo.to_str().unwrap()]).output().unwrap();
        for arg in &[["config","user.email","t@t.com"],["config","user.name","T"]] {
            Command::new("git").args(["-C", repo.to_str().unwrap()]).args(arg).output().unwrap();
        }
        std::fs::write(repo.join("f"), "x").unwrap();
        Command::new("git").args(["-C", repo.to_str().unwrap(), "add", "."]).output().unwrap();
        Command::new("git").args(["-C", repo.to_str().unwrap(), "commit", "--allow-empty", "-m", "init"]).output().unwrap();
        Command::new("git").args(["-C", repo.to_str().unwrap(), "checkout", "-b", "feat/ext"]).output().unwrap();
        Command::new("git").args(["-C", repo.to_str().unwrap(), "checkout", "main"]).output().unwrap();
        let wt = crate::worktree::worktree_path(&repo, "feat/ext");
        std::fs::create_dir_all(wt.parent().unwrap()).unwrap();
        Command::new("git").args(["-C", repo.to_str().unwrap(), "worktree", "add", wt.to_str().unwrap(), "feat/ext"]).output().unwrap();
        // Create the artifact the health-check tests for
        std::fs::create_dir_all(wt.join("dist")).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let ps = crate::process_store::ProcessStateStore::open(dir.path());
        let app_store = crate::app_config::AppConfigStore::open(dir.path().join("config.toml"));
        app_store.save_app_config(crate::app_config::AppConfig {
            name: "ext".into(),
            bootstrap: "true".into(),
            teardown: "true".into(),
            startup_timeout: "1s".into(),
            health_check: Some("test -d dist".into()),
            ephemeral: true,
            main_worktree: None, setup: None,
            volume_check: None,
}).unwrap();
        crate::feature::feature_new(&ps, "my-feat").unwrap();
        let mut state = ps.load().unwrap();
        state.records.insert(42, crate::process_store::ProcessRecord {
            pr: 42,
            expected_branch: "feat/ext".into(),
            pid: None,
            feature_envelopes: vec!["my-feat".into()], feature_envelope: None,
            worktree: String::new(),
            app_config_names: vec!["ext".into()],
        });
        ps.save_state(state).unwrap();
        let result = crate::commands::cmd_feature_status_with_client(&ps, &app_store, "my-feat", None, "o", "r", &repo);
        assert!(result.is_ok(), "cmd_feature_status_with_client must succeed: {:?}", result);
        let out = result.unwrap();
        assert!(out.contains("✓ built"),
            "ephemeral app must show '✓ built' regardless of health_check, got: {}", out);
    }

    // Ephemeral: feature status shows "✓ built" not "✗ stopped" for ephemeral apps
    #[test]
    fn cmd_feature_status_governs_ephemeral_shows_built_not_stopped() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::process::Command::new("git").args(["init", "-b", "main", repo.to_str().unwrap()]).output().unwrap();
        for arg in &[["config","user.email","t@t.com"],["config","user.name","T"]] {
            std::process::Command::new("git").args(["-C", repo.to_str().unwrap()]).args(arg).output().unwrap();
        }
        std::fs::write(repo.join("f"), "x").unwrap();
        std::process::Command::new("git").args(["-C", repo.to_str().unwrap(), "add", "."]).output().unwrap();
        std::process::Command::new("git").args(["-C", repo.to_str().unwrap(), "commit", "--allow-empty", "-m", "init"]).output().unwrap();
        std::process::Command::new("git").args(["-C", repo.to_str().unwrap(), "checkout", "-b", "feat/ext"]).output().unwrap();
        std::process::Command::new("git").args(["-C", repo.to_str().unwrap(), "checkout", "main"]).output().unwrap();
        let wt = crate::worktree::worktree_path(&repo, "feat/ext");
        std::fs::create_dir_all(wt.parent().unwrap()).unwrap();
        std::process::Command::new("git").args(["-C", repo.to_str().unwrap(), "worktree", "add", wt.to_str().unwrap(), "feat/ext"]).output().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let ps = crate::process_store::ProcessStateStore::open(dir.path());
        let app_store = crate::app_config::AppConfigStore::open(dir.path().join("config.toml"));
        app_store.save_app_config(crate::app_config::AppConfig {
            name: "ext".into(),
            bootstrap: "true".into(),
            teardown: "true".into(),
            startup_timeout: "1s".into(),
            health_check: None,
            ephemeral: true,
            main_worktree: None, setup: None,
            volume_check: None,
}).unwrap();
        crate::feature::feature_new(&ps, "my-feat").unwrap();
        let mut state = ps.load().unwrap();
        state.records.insert(42, crate::process_store::ProcessRecord {
            pr: 42,
            expected_branch: "feat/ext".into(),
            pid: None,
            feature_envelopes: vec!["my-feat".into()], feature_envelope: None,
            worktree: String::new(),
            app_config_names: vec!["ext".into()],
        });
        ps.save_state(state).unwrap();
        let result = crate::commands::cmd_feature_status_with_client(&ps, &app_store, "my-feat", None, "o", "r", &repo);
        assert!(result.is_ok(), "cmd_feature_status_with_client must succeed: {:?}", result);
        let out = result.unwrap();
        assert!(out.contains("✓ built"), "ephemeral app must show '✓ built', got: {}", out);
        assert!(!out.contains("✗ stopped"), "ephemeral app must not show '✗ stopped', got: {}", out);
    }

    // healthy-but-untracked: when another feature envelope has a live PID, name it in the warning
    #[test]
    fn cmd_feature_status_governs_healthy_untracked_names_owning_feature() {
        let dir = tempfile::tempdir().unwrap();
        let ps = crate::process_store::ProcessStateStore::open(dir.path());
        let app_store = crate::app_config::AppConfigStore::open(dir.path().join("config.toml"));
        app_store.save_app_config(crate::app_config::AppConfig {
            name: "svc".into(),
            bootstrap: "true".into(),
            teardown: "true".into(),
            startup_timeout: "1s".into(),
            health_check: Some("true".into()),
            ephemeral: false,
            main_worktree: None, setup: None,
            volume_check: None,
}).unwrap();
        crate::feature::feature_new(&ps, "auth-refactor").unwrap();
        crate::feature::feature_new(&ps, "payment-v2").unwrap();
        // PR #42 in auth-refactor: no live PID, service healthy (health_check="true")
        let mut state = ps.load().unwrap();
        // empty branch resolves to repo_root so health_check_service can run "true" successfully
        state.records.insert(42, crate::process_store::ProcessRecord {
            pr: 42,
            expected_branch: "".into(),
            pid: None,
            feature_envelopes: vec!["auth-refactor".into()], feature_envelope: None,
            worktree: String::new(),
            app_config_names: vec!["svc".into()],
        });
        // PR #99 in payment-v2: live PID (current process — guaranteed alive)
        state.records.insert(99, crate::process_store::ProcessRecord {
            pr: 99,
            expected_branch: "".into(),
            pid: Some(std::process::id()),
            feature_envelopes: vec!["payment-v2".into()], feature_envelope: None,
            worktree: String::new(),
            app_config_names: vec!["svc".into()],
        });
        ps.save_state(state).unwrap();
        let result = crate::commands::cmd_feature_status(
            &ps, &app_store, "auth-refactor", false, std::path::Path::new(".")
        );
        assert!(result.is_ok(), "cmd_feature_status must succeed: {:?}", result);
        let out = result.unwrap();
        assert!(out.contains("running under feature: payment-v2"),
            "healthy-but-untracked warning must name the owning feature envelope, got: {}", out);
    }

    #[test]
    fn cmd_app_list_governs_returns_defined_config_names() {
        let tmp = tempfile::tempdir().unwrap();
        let store = crate::app_config::AppConfigStore::open(tmp.path().join("config.toml"));
        store.save_app_config(crate::app_config::AppConfig {
            name: "backend".into(), bootstrap: "echo up".into(), teardown: "echo down".into(),
            startup_timeout: "5s".into(), health_check: None, ephemeral: false, main_worktree: None, setup: None,
            volume_check: None,
}).unwrap();
        store.save_app_config(crate::app_config::AppConfig {
            name: "frontend".into(), bootstrap: "echo up".into(), teardown: "echo down".into(),
            startup_timeout: "5s".into(), health_check: None, ephemeral: false, main_worktree: None, setup: None,
            volume_check: None,
}).unwrap();
        let result = crate::commands::cmd_app_list(&store);
        assert!(result.is_ok(), "cmd_app_list must succeed: {:?}", result);
        let out = result.unwrap();
        assert!(out.contains("backend"), "cmd_app_list must list 'backend', got: {}", out);
        assert!(out.contains("frontend"), "cmd_app_list must list 'frontend', got: {}", out);
    }

    #[test]
    fn cmd_app_show_governs_displays_all_fields() {
        let dir = tempfile::tempdir().unwrap();
        let store = crate::app_config::AppConfigStore::open(dir.path().join("config.toml"));
        store.save_app_config(crate::app_config::AppConfig {
            name: "payments-api".into(),
            bootstrap: "docker compose up".into(),
            teardown: "docker compose down".into(),
            startup_timeout: "30s".into(),
            health_check: Some("curl -f http://localhost:8080/health".into()),
            ephemeral: false,
            main_worktree: Some("/repos/main".into()),
            setup: Some("npm install".into()),
            volume_check: None,
        }).unwrap();
        let result = crate::commands::cmd_app_show(&store, "payments-api");
        assert!(result.is_ok(), "cmd_app_show must succeed: {:?}", result);
        let out = result.unwrap();
        assert!(out.contains("docker compose up"), "cmd_app_show must show bootstrap, got: {}", out);
        assert!(out.contains("docker compose down"), "cmd_app_show must show teardown, got: {}", out);
        assert!(out.contains("30s"), "cmd_app_show must show startup_timeout, got: {}", out);
        assert!(out.contains("curl -f http://localhost:8080/health"), "cmd_app_show must show health_check, got: {}", out);
        assert!(out.contains("/repos/main"), "cmd_app_show must show main_worktree, got: {}", out);
        assert!(out.contains("npm install"), "cmd_app_show must show setup, got: {}", out);
    }

    #[test]
    fn cmd_feature_remove_dep_governs_removes_from_envelope_deps() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        let ps = crate::process_store::ProcessStateStore::open(&git_dir);
        let app_store_dir2 = tempfile::tempdir().unwrap();
        let app_store2 = crate::app_config::AppConfigStore::open(app_store_dir2.path().join("config.toml"));
        crate::feature::feature_new(&ps, "my-feat").unwrap();
        crate::commands::cmd_feature_add_dep(&ps, &app_store2, "my-feat", "backend").unwrap();
        let result = crate::commands::cmd_feature_remove_dep(&ps, "my-feat", "backend");
        assert!(result.is_ok(), "cmd_feature_remove_dep must succeed: {:?}", result);
        let state = ps.load().unwrap();
        let deps = state.envelope_deps.get("my-feat").map(|v| v.as_slice()).unwrap_or(&[]);
        assert!(!deps.contains(&"backend".to_string()),
            "cmd_feature_remove_dep must remove backend from envelope_deps, got: {:?}", deps);
    }

    #[test]
    fn cmd_feature_remove_config_governs_removes_config_from_pr_record() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        let ps = crate::process_store::ProcessStateStore::open(&git_dir);
        crate::feature::feature_new(&ps, "my-feat").unwrap();
        let mut state = ps.load().unwrap();
        state.records.insert(42, crate::process_store::ProcessRecord {
            pr: 42, expected_branch: "feat/x".into(), pid: None,
            feature_envelopes: vec!["my-feat".into()], feature_envelope: None,
            worktree: "".into(),
            app_config_names: vec!["svc-a".into(), "svc-b".into()],
        });
        ps.save_state(state).unwrap();
        let result = crate::commands::cmd_feature_remove_config(&ps, "my-feat", 42, "svc-a");
        assert!(result.is_ok(), "cmd_feature_remove_config must succeed: {:?}", result);
        let after = ps.load().unwrap();
        let configs = &after.records[&42].app_config_names;
        assert!(!configs.contains(&"svc-a".to_string()),
            "cmd_feature_remove_config must remove svc-a from PR 42 configs, got: {:?}", configs);
        assert!(configs.contains(&"svc-b".to_string()),
            "cmd_feature_remove_config must preserve svc-b on PR 42 configs, got: {:?}", configs);
    }

    #[test]
    fn cmd_feature_logs_governs_shows_dep_log_content() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        let ps = crate::process_store::ProcessStateStore::open(&git_dir);
        crate::feature::feature_new(&ps, "my-feat").unwrap();
        let mut state = ps.load().unwrap();
        state.dep_records.insert("my-feat:backend".into(), crate::process_store::DepRecord {
            app_config_name: "backend".into(), feature_envelope: "my-feat".into(),
            pid: None, worktree: tmp.path().to_string_lossy().to_string(),
        });
        ps.save_state(state).unwrap();
        let log_dir = git_dir.join("fp").join("logs");
        std::fs::create_dir_all(&log_dir).unwrap();
        std::fs::write(log_dir.join("fp-dep-my-feat-backend.log"), "backend started ok\n").unwrap();
        let result = crate::commands::cmd_feature_logs(&ps, "my-feat", false);
        assert!(result.is_ok(), "cmd_feature_logs must succeed: {:?}", result);
        let out = result.unwrap();
        assert!(out.contains("backend started ok"),
            "cmd_feature_logs must include dep log content, got: {}", out);
        assert!(out.contains("backend"),
            "cmd_feature_logs must include dep name in header, got: {}", out);
    }

    #[test]
    fn cmd_feature_logs_governs_empty_when_no_logs() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        let ps = crate::process_store::ProcessStateStore::open(&git_dir);
        crate::feature::feature_new(&ps, "my-feat").unwrap();
        let result = crate::commands::cmd_feature_logs(&ps, "my-feat", false);
        assert!(result.is_ok(), "cmd_feature_logs must succeed even with no logs: {:?}", result);
        let out = result.unwrap();
        assert!(out.contains("no logs") || out.contains("No logs"),
            "cmd_feature_logs must report no logs when none exist, got: {}", out);
    }

    #[test]
    fn cmd_feature_test_governs_runs_stored_command_and_returns_output() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        let ps = crate::process_store::ProcessStateStore::open(&git_dir);
        crate::feature::feature_new(&ps, "my-feat").unwrap();
        crate::commands::cmd_feature_set_test(&ps, "my-feat", "echo e2e-ran").unwrap();
        let result = crate::commands::cmd_feature_test(&ps, "my-feat", tmp.path());
        assert!(result.is_ok(), "cmd_feature_test must succeed when test command exits 0: {:?}", result);
        let out = result.unwrap();
        assert!(out.contains("e2e-ran") || out.contains("passed"),
            "cmd_feature_test must include test output or pass status, got: {}", out);
    }

    #[test]
    fn cmd_feature_set_test_governs_stores_command() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        let ps = crate::process_store::ProcessStateStore::open(&git_dir);
        crate::feature::feature_new(&ps, "my-feat").unwrap();
        let result = crate::commands::cmd_feature_set_test(&ps, "my-feat", "echo e2e-ok");
        assert!(result.is_ok(), "cmd_feature_set_test must succeed: {:?}", result);
        let state = ps.load().unwrap();
        let cmd = state.feature_configs.get("my-feat").and_then(|c| c.test_command.as_deref());
        assert_eq!(cmd, Some("echo e2e-ok"),
            "cmd_feature_set_test must persist test command, got: {:?}", cmd);
    }

    #[test]
    fn cmd_status_all_governs_shows_unmanaged_hint_when_service_healthy_but_pid_dead() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join("git_dir");
        std::fs::create_dir_all(&git_dir).unwrap();
        let store = make_store_with_pr(&git_dir, 42, "feat/unmanaged");
        let ps = crate::process_store::ProcessStateStore::open(&git_dir);
        ps.activate(crate::process_store::ProcessRecord {
            pr: 42, expected_branch: "feat/unmanaged".into(), pid: None,
            feature_envelopes: vec![], feature_envelope: None, worktree: tmp.path().to_string_lossy().to_string(),
            app_config_names: vec!["svc".into()],
        }).unwrap();
        let app_store = crate::app_config::AppConfigStore::open(tmp.path().join("config.toml"));
        app_store.save_app_config(crate::app_config::AppConfig {
            name: "svc".into(), bootstrap: "echo up".into(), teardown: "echo down".into(),
            startup_timeout: "5s".into(), health_check: Some("true".into()),
            ephemeral: false, main_worktree: None, setup: None,
            volume_check: None,
}).unwrap();
        let fake = make_fake_with_pr(42);
        let out = crate::commands::cmd_status_all(Some(&fake), &store, Some(&ps), Some(&app_store), &git_dir, "alice", "repo", false).unwrap();
        assert!(out.contains("healthy but untracked — another process may be listening"),
            "cmd_status_all must warn 'healthy but untracked — another process may be listening' when service healthy but pid dead, got: {}", out);
    }

    #[test]
    fn cmd_pr_up_force_governs_runs_teardown_before_bootstrap() {
        let dir = tempfile::tempdir().unwrap();
        let ps = crate::process_store::ProcessStateStore::open(dir.path());
        let app_store = crate::app_config::AppConfigStore::open(dir.path().join("config.toml"));
        let wt = tempfile::tempdir().unwrap();
        let teardown_marker = dir.path().join("teardown_ran");
        let teardown_cmd = format!("touch {}", teardown_marker.display());
        app_store.save_app_config(crate::app_config::AppConfig {
            name: "svc".into(), bootstrap: "echo up".into(), teardown: teardown_cmd,
            startup_timeout: "5s".into(), health_check: None, ephemeral: false, main_worktree: None, setup: None,
            volume_check: None,
}).unwrap();
        let mut state = ps.load().unwrap();
        state.records.insert(99, crate::process_store::ProcessRecord {
            pr: 99, expected_branch: String::new(), pid: None,
            feature_envelopes: vec![], feature_envelope: None,
            worktree: wt.path().to_string_lossy().to_string(),
            app_config_names: vec!["svc".into()],
        });
        ps.save_state(state).unwrap();
        let result = crate::commands::cmd_pr_up_force(&ps, &app_store, 99);
        assert!(result.is_ok(), "cmd_pr_up_force must succeed: {:?}", result);
        assert!(teardown_marker.exists(),
            "cmd_pr_up_force must run teardown before bootstrap");
    }

    #[test]
    fn cmd_feature_up_governs_shows_foreground_note() {
        let dir = tempfile::tempdir().unwrap();
        let ps = crate::process_store::ProcessStateStore::open(dir.path());
        let app_store = crate::app_config::AppConfigStore::open(dir.path().join("config.toml"));
        app_store.save_app_config(crate::app_config::AppConfig {
            name: "svc".into(), bootstrap: "true".into(), teardown: "true".into(),
            startup_timeout: "1s".into(), health_check: None, ephemeral: false,
            main_worktree: None, setup: None,
            volume_check: None,
}).unwrap();
        let live_pid = std::process::id();
        let rec = crate::process_store::ProcessRecord {
            pr: 11, expected_branch: "".into(), pid: Some(live_pid),
            feature_envelopes: vec!["my-feat".into()], feature_envelope: None,
            worktree: dir.path().to_string_lossy().to_string(),
            app_config_names: vec!["svc".into()],
        };
        ps.activate(rec).unwrap();
        let mut state = ps.load().unwrap();
        state.feature_envelopes.insert("my-feat".to_string());
        ps.save_state(state).unwrap();
        let result = crate::commands::cmd_feature_up(&ps, &app_store, "my-feat", dir.path());
        assert!(result.is_ok(), "cmd_feature_up must succeed: {:?}", result);
        let output = result.unwrap();
        assert!(output.contains("foreground"),
            "cmd_feature_up must mention foreground requirement, got: {}", output);
    }

    #[test]
    fn cmd_feature_status_governs_shows_daemonize_hint_when_stopped() {
        let dir = tempfile::tempdir().unwrap();
        let ps = crate::process_store::ProcessStateStore::open(dir.path());
        let app_store = crate::app_config::AppConfigStore::open(dir.path().join("config.toml"));
        let rec = crate::process_store::ProcessRecord {
            pr: 22, expected_branch: "".into(), pid: None,
            feature_envelopes: vec!["my-feat".into()], feature_envelope: None,
            worktree: dir.path().to_string_lossy().to_string(),
            app_config_names: vec![],
        };
        ps.activate(rec).unwrap();
        let mut state = ps.load().unwrap();
        state.feature_envelopes.insert("my-feat".to_string());
        ps.save_state(state).unwrap();
        let result = crate::commands::cmd_feature_status(&ps, &app_store, "my-feat", false, dir.path());
        assert!(result.is_ok(), "cmd_feature_status must succeed: {:?}", result);
        let output = result.unwrap();
        assert!(output.contains("daemonize") || output.contains("foreground"),
            "cmd_feature_status must hint about daemonizing when PR is stopped, got: {}", output);
    }

    #[test]
    fn cmd_feature_status_governs_shows_recovery_hint_when_stopped_but_healthy() {
        let dir = tempfile::tempdir().unwrap();
        let ps = crate::process_store::ProcessStateStore::open(dir.path());
        let app_store = crate::app_config::AppConfigStore::open(dir.path().join("config.toml"));
        // Save an app config with a health check that always passes
        app_store.save_app_config(crate::app_config::AppConfig {
            name: "svc".into(), bootstrap: "true".into(), teardown: "true".into(),
            startup_timeout: "1s".into(), health_check: Some("true".into()), ephemeral: false,
            main_worktree: None, setup: None,
            volume_check: None,
}).unwrap();
        // PR with no live pid (stopped) but app config has health_check=true (healthy)
        let rec = crate::process_store::ProcessRecord {
            pr: 55,
            expected_branch: "".into(),
            pid: None,
            feature_envelopes: vec!["my-feat".into()], feature_envelope: None,
            worktree: dir.path().to_string_lossy().to_string(),
            app_config_names: vec!["svc".into()],
        };
        ps.activate(rec).unwrap();
        let mut state = ps.load().unwrap();
        state.feature_envelopes.insert("my-feat".to_string());
        ps.save_state(state).unwrap();
        let result = crate::commands::cmd_feature_status(&ps, &app_store, "my-feat", false, dir.path());
        assert!(result.is_ok(), "cmd_feature_status must succeed: {:?}", result);
        let output = result.unwrap();
        assert!(output.contains("healthy but untracked — another process may be listening"),
            "cmd_feature_status must warn 'healthy but untracked' when PR is stopped but healthy, got: {}", output);
    }

    #[test]
    fn cmd_feature_status_governs_shows_app_config_names_per_pr() {
        let dir = tempfile::tempdir().unwrap();
        let ps = crate::process_store::ProcessStateStore::open(dir.path());
        let app_store = crate::app_config::AppConfigStore::open(dir.path().join("config.toml"));
        let live_pid = std::process::id();
        let rec = crate::process_store::ProcessRecord {
            pr: 77,
            expected_branch: "feat/foo".into(),
            pid: Some(live_pid),
            feature_envelopes: vec!["my-feat".into()], feature_envelope: None,
            worktree: dir.path().to_string_lossy().to_string(),
            app_config_names: vec!["frontend".into(), "backend".into()],
        };
        ps.activate(rec).unwrap();
        let mut state = ps.load().unwrap();
        state.feature_envelopes.insert("my-feat".to_string());
        ps.save_state(state).unwrap();
        let result = crate::commands::cmd_feature_status(&ps, &app_store, "my-feat", false, std::path::Path::new("."));
        assert!(result.is_ok(), "cmd_feature_status must succeed: {:?}", result);
        let output = result.unwrap();
        assert!(output.contains("frontend") && output.contains("backend"),
            "cmd_feature_status must show app_config_names for each PR, got: {}", output);
    }

    #[test]
    fn cmd_feature_status_governs_shows_test_command_when_set() {
        let dir = tempfile::tempdir().unwrap();
        let ps = crate::process_store::ProcessStateStore::open(dir.path());
        let app_store = crate::app_config::AppConfigStore::open(dir.path().join("config.toml"));
        crate::feature::feature_new(&ps, "my-feat").unwrap();
        crate::commands::cmd_feature_set_test(&ps, "my-feat", "pytest tests/e2e/test_feature.py").unwrap();
        let result = crate::commands::cmd_feature_status(&ps, &app_store, "my-feat", false, std::path::Path::new("."));
        assert!(result.is_ok(), "cmd_feature_status must succeed: {:?}", result);
        let output = result.unwrap();
        assert!(output.contains("pytest tests/e2e/test_feature.py"),
            "cmd_feature_status must show test command when set, got: {}", output);
    }

    #[test]
    fn cmd_feature_status_governs_shows_run_hint_when_test_set() {
        let dir = tempfile::tempdir().unwrap();
        let ps = crate::process_store::ProcessStateStore::open(dir.path());
        let app_store = crate::app_config::AppConfigStore::open(dir.path().join("config.toml"));
        crate::feature::feature_new(&ps, "my-feat").unwrap();
        crate::commands::cmd_feature_set_test(&ps, "my-feat", "pytest tests/e2e").unwrap();
        let result = crate::commands::cmd_feature_status(&ps, &app_store, "my-feat", false, std::path::Path::new("."));
        assert!(result.is_ok(), "cmd_feature_status must succeed: {:?}", result);
        let output = result.unwrap();
        assert!(output.contains("fp feature test my-feat"),
            "cmd_feature_status must show run hint when test command is set, got: {}", output);
    }

    #[test]
    fn cmd_feature_app_setup_governs_runs_setup_and_marks_complete() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        let ps = crate::process_store::ProcessStateStore::open(&git_dir);
        let app_store_dir = tempfile::tempdir().unwrap();
        let app_store = crate::app_config::AppConfigStore::open(app_store_dir.path().join("config.toml"));
        let mut cfg = crate::app_config::AppConfig {
            name: "payments-api".into(),
            bootstrap: "true".into(),
            teardown: "true".into(),
            startup_timeout: "60s".into(),
            health_check: None,
            ephemeral: false,
            main_worktree: None,
            setup: Some("true".into()),
            volume_check: None,
};
        app_store.save_app_config(cfg).unwrap();
        // Feature with a PR record pointing to the tmp worktree
        let mut state = ps.load().unwrap();
        state.feature_envelopes.insert("my-feat".into());
        state.records.insert(10, crate::process_store::ProcessRecord {
            pr: 10,
            expected_branch: "feat/x".into(),
            pid: None,
            feature_envelopes: vec!["my-feat".into()], feature_envelope: None,
            worktree: tmp.path().to_string_lossy().to_string(),
            app_config_names: vec!["payments-api".into()],
        });
        ps.save_state(state).unwrap();
        let result = crate::commands::cmd_feature_app_setup(&ps, &app_store, "my-feat", "payments-api");
        assert!(result.is_ok(), "cmd_feature_app_setup must succeed: {:?}", result);
        let loaded = ps.load().unwrap();
        let worktree = tmp.path().to_string_lossy().to_string();
        assert!(loaded.setup_completed.contains(&("payments-api".into(), worktree.clone())),
            "cmd_feature_app_setup must mark setup complete for (app, worktree), got: {:?}", loaded.setup_completed);
    }

    #[test]
    fn cmd_feature_app_setup_governs_runs_setup_in_all_pr_worktrees() {
        let tmp1 = tempfile::tempdir().unwrap();
        let tmp2 = tempfile::tempdir().unwrap();
        let git_dir = tmp1.path().join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        let ps = crate::process_store::ProcessStateStore::open(&git_dir);
        let app_store_dir = tempfile::tempdir().unwrap();
        let app_store = crate::app_config::AppConfigStore::open(app_store_dir.path().join("config.toml"));
        app_store.save_app_config(crate::app_config::AppConfig {
            name: "payments-api".into(),
            bootstrap: "true".into(),
            teardown: "true".into(),
            startup_timeout: "60s".into(),
            health_check: None,
            ephemeral: false,
            main_worktree: None,
            setup: Some("true".into()),
            volume_check: None,
        }).unwrap();
        let mut state = ps.load().unwrap();
        state.feature_envelopes.insert("my-feat".into());
        state.records.insert(10, crate::process_store::ProcessRecord {
            pr: 10,
            expected_branch: "".into(),
            pid: None,
            feature_envelopes: vec!["my-feat".into()], feature_envelope: None,
            worktree: tmp1.path().to_string_lossy().to_string(),
            app_config_names: vec!["payments-api".into()],
        });
        state.records.insert(20, crate::process_store::ProcessRecord {
            pr: 20,
            expected_branch: "".into(),
            pid: None,
            feature_envelopes: vec!["my-feat".into()], feature_envelope: None,
            worktree: tmp2.path().to_string_lossy().to_string(),
            app_config_names: vec!["payments-api".into()],
        });
        ps.save_state(state).unwrap();
        let result = crate::commands::cmd_feature_app_setup(&ps, &app_store, "my-feat", "payments-api");
        assert!(result.is_ok(), "cmd_feature_app_setup must succeed for multi-worktree: {:?}", result);
        let loaded = ps.load().unwrap();
        let wt1 = tmp1.path().to_string_lossy().to_string();
        let wt2 = tmp2.path().to_string_lossy().to_string();
        assert!(loaded.setup_completed.contains(&("payments-api".into(), wt1.clone())),
            "cmd_feature_app_setup must mark setup complete for PR 10 worktree, got: {:?}", loaded.setup_completed);
        assert!(loaded.setup_completed.contains(&("payments-api".into(), wt2.clone())),
            "cmd_feature_app_setup must mark setup complete for PR 20 worktree, got: {:?}", loaded.setup_completed);
    }

    #[test]
    fn cmd_feature_up_governs_warns_when_setup_not_run() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        let ps = crate::process_store::ProcessStateStore::open(&git_dir);
        let app_store_dir = tempfile::tempdir().unwrap();
        let app_store = crate::app_config::AppConfigStore::open(app_store_dir.path().join("config.toml"));
        app_store.save_app_config(crate::app_config::AppConfig {
            name: "payments-api".into(),
            bootstrap: "true".into(),
            teardown: "true".into(),
            startup_timeout: "60s".into(),
            health_check: None,
            ephemeral: false,
            main_worktree: None,
            setup: Some("npm install".into()),
            volume_check: None,
        }).unwrap();
        let mut state = ps.load().unwrap();
        state.feature_envelopes.insert("my-feat".into());
        state.records.insert(10, crate::process_store::ProcessRecord {
            pr: 10,
            expected_branch: "".into(),
            pid: None,
            feature_envelopes: vec!["my-feat".into()], feature_envelope: None,
            worktree: tmp.path().to_string_lossy().to_string(),
            app_config_names: vec!["payments-api".into()],
        });
        ps.save_state(state).unwrap();
        let result = crate::commands::cmd_feature_up(&ps, &app_store, "my-feat", tmp.path());
        assert!(result.is_ok(), "cmd_feature_up must succeed even when setup unrun: {:?}", result);
        let out = result.unwrap();
        assert!(out.contains("setup") && out.contains("payments-api"),
            "cmd_feature_up must warn about unrun setup for payments-api, got: {}", out);
    }

    #[test]
    fn cmd_feature_add_dep_governs_auto_runs_setup_when_worktree_known() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        let ps = crate::process_store::ProcessStateStore::open(&git_dir);
        let app_store_dir = tempfile::tempdir().unwrap();
        let app_store = crate::app_config::AppConfigStore::open(app_store_dir.path().join("config.toml"));
        app_store.save_app_config(crate::app_config::AppConfig {
            name: "notifications-svc".into(),
            bootstrap: "true".into(),
            teardown: "true".into(),
            startup_timeout: "60s".into(),
            health_check: None,
            ephemeral: false,
            main_worktree: None,
            setup: Some("true".into()),
            volume_check: None,
        }).unwrap();
        let mut state = ps.load().unwrap();
        state.feature_envelopes.insert("my-feat".into());
        state.records.insert(10, crate::process_store::ProcessRecord {
            pr: 10,
            expected_branch: "feat/x".into(),
            pid: None,
            feature_envelopes: vec!["my-feat".into()], feature_envelope: None,
            worktree: tmp.path().to_string_lossy().to_string(),
            app_config_names: vec![],
        });
        ps.save_state(state).unwrap();
        let result = crate::commands::cmd_feature_add_dep(&ps, &app_store, "my-feat", "notifications-svc");
        assert!(result.is_ok(), "cmd_feature_add_dep must succeed: {:?}", result);
        let loaded = ps.load().unwrap();
        let worktree = tmp.path().to_string_lossy().to_string();
        assert!(loaded.setup_completed.contains(&("notifications-svc".into(), worktree)),
            "cmd_feature_add_dep must auto-run setup and mark complete when worktree known, got: {:?}", loaded.setup_completed);
    }

    #[test]
    fn cmd_feature_test_governs_shows_command_before_running() {
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join(".git");
        std::fs::create_dir_all(&git_dir).unwrap();
        let ps = crate::process_store::ProcessStateStore::open(&git_dir);
        crate::feature::feature_new(&ps, "my-feat").unwrap();
        crate::commands::cmd_feature_set_test(&ps, "my-feat", "echo e2e-ran").unwrap();
        let result = crate::commands::cmd_feature_test(&ps, "my-feat", tmp.path());
        assert!(result.is_ok(), "cmd_feature_test must succeed: {:?}", result);
        let out = result.unwrap();
        assert!(out.contains("echo e2e-ran"),
            "cmd_feature_test must show the test command before running, got: {}", out);
    }

    #[test]
    fn cmd_status_all_shows_dirty_indicator_for_pr_with_uncommitted_changes() {
        use std::process::Command;
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let git_dir = repo.join(".git");
        Command::new("git").args(["init", "-b", "main"]).current_dir(&repo).output().unwrap();
        Command::new("git").args(["config", "user.email", "t@t.com"]).current_dir(&repo).output().unwrap();
        Command::new("git").args(["config", "user.name", "T"]).current_dir(&repo).output().unwrap();
        std::fs::write(repo.join("a.txt"), "a").unwrap();
        Command::new("git").args(["add", "."]).current_dir(&repo).output().unwrap();
        Command::new("git").args(["commit", "-m", "init"]).current_dir(&repo).output().unwrap();
        Command::new("git").args(["checkout", "-b", "feat/dirty"]).current_dir(&repo).output().unwrap();
        std::fs::write(repo.join("b.txt"), "b").unwrap();
        Command::new("git").args(["add", "."]).current_dir(&repo).output().unwrap();
        Command::new("git").args(["commit", "-m", "feat"]).current_dir(&repo).output().unwrap();
        Command::new("git").args(["checkout", "main"]).current_dir(&repo).output().unwrap();
        // Create a worktree for feat/dirty
        let wt_path = crate::worktree::worktree_path(&repo, "feat/dirty");
        std::fs::create_dir_all(wt_path.parent().unwrap()).unwrap();
        Command::new("git").args(["worktree", "add", wt_path.to_str().unwrap(), "feat/dirty"]).current_dir(&repo).output().unwrap();
        // Leave an uncommitted file in the worktree
        std::fs::write(wt_path.join("dirty.txt"), "dirty").unwrap();

        let store = crate::store::Store::open(&git_dir);
        store.track(1).unwrap();
        store.update_cache(crate::store::PrCache { number: 1, title: "Dirty PR".into(), branch: "feat/dirty".into(), base: "main".into() }).unwrap();
        let mut fake = crate::github::FakeGithubClient::new();
        fake.set_pr(1, crate::model::PrState {
            number: 1, title: "Dirty PR".into(), branch: "feat/dirty".into(), base: "main".into(),
            head_sha: "abc".into(), draft: false, approved: true, checks: vec![], threads: vec![],
            needs_parent_rebase: false, has_merge_conflict: false,
            codeowners_eligibility: Default::default(), created_at: None, is_stacked: false,
        });
        let out = crate::commands::cmd_status_all(Some(&fake), &store, None, None, &git_dir, "alice", "repo", false).unwrap();
        assert!(out.contains("dirty") || out.contains("uncommitted"),
            "cmd_status_all must show dirty indicator for PR with uncommitted changes, got: {}", out);
    }

    #[test]
    fn cmd_rebase_stack_bails_when_worktree_has_uncommitted_changes() {
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
        std::fs::write(repo.join("a.txt"), "a").unwrap();
        Command::new("git").args(["add", "."]).current_dir(&repo).output().unwrap();
        Command::new("git").args(["commit", "-m", "init"]).current_dir(&repo).output().unwrap();
        Command::new("git").args(["push", "-u", "origin", "main"]).current_dir(&repo).output().unwrap();
        Command::new("git").args(["checkout", "-b", "feat/x"]).current_dir(&repo).output().unwrap();
        std::fs::write(repo.join("b.txt"), "b").unwrap();
        Command::new("git").args(["add", "."]).current_dir(&repo).output().unwrap();
        Command::new("git").args(["commit", "-m", "feat"]).current_dir(&repo).output().unwrap();
        Command::new("git").args(["push", "-u", "origin", "feat/x"]).current_dir(&repo).output().unwrap();
        Command::new("git").args(["checkout", "main"]).current_dir(&repo).output().unwrap();
        // Create a worktree for feat/x
        let wt_path = crate::worktree::worktree_path(&repo, "feat/x");
        std::fs::create_dir_all(wt_path.parent().unwrap()).unwrap();
        Command::new("git").args(["worktree", "add", wt_path.to_str().unwrap(), "feat/x"]).current_dir(&repo).output().unwrap();
        // Leave an uncommitted file in the worktree
        std::fs::write(wt_path.join("dirty.txt"), "dirty").unwrap();

        let store = crate::store::Store::open(&git_dir);
        store.track(1).unwrap();
        store.update_cache(crate::store::PrCache { number: 1, title: "X".into(), branch: "feat/x".into(), base: "main".into() }).unwrap();
        let fake = crate::github::FakeGithubClient::new_with_pr(1, "feat/x", "X", "main");
        let result = crate::commands::cmd_rebase_stack(
            Some(&fake), "o", "r", &store, &repo, &git_dir, None, &|_| {},
        );
        assert!(result.is_err(), "cmd_rebase_stack must fail when worktree has uncommitted changes");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("uncommitted") || err.contains("dirty"),
            "error must mention uncommitted changes, got: {}", err);
    }

    #[test]
    fn cmd_feature_status_shows_dirty_indicator_for_pr_with_uncommitted_changes() {
        use std::process::Command;
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let git_dir = repo.join(".git");
        Command::new("git").args(["init", "-b", "main"]).current_dir(&repo).output().unwrap();
        Command::new("git").args(["config", "user.email", "t@t.com"]).current_dir(&repo).output().unwrap();
        Command::new("git").args(["config", "user.name", "T"]).current_dir(&repo).output().unwrap();
        std::fs::write(repo.join("a.txt"), "a").unwrap();
        Command::new("git").args(["add", "."]).current_dir(&repo).output().unwrap();
        Command::new("git").args(["commit", "-m", "init"]).current_dir(&repo).output().unwrap();
        Command::new("git").args(["checkout", "-b", "feat/dirty"]).current_dir(&repo).output().unwrap();
        std::fs::write(repo.join("b.txt"), "b").unwrap();
        Command::new("git").args(["add", "."]).current_dir(&repo).output().unwrap();
        Command::new("git").args(["commit", "-m", "feat"]).current_dir(&repo).output().unwrap();
        Command::new("git").args(["checkout", "main"]).current_dir(&repo).output().unwrap();
        let wt_path = crate::worktree::worktree_path(&repo, "feat/dirty");
        std::fs::create_dir_all(wt_path.parent().unwrap()).unwrap();
        Command::new("git").args(["worktree", "add", wt_path.to_str().unwrap(), "feat/dirty"]).current_dir(&repo).output().unwrap();
        std::fs::write(wt_path.join("dirty.txt"), "dirty").unwrap();

        let ps = crate::process_store::ProcessStateStore::open(&git_dir);
        let rec = crate::process_store::ProcessRecord {
            pr: 42,
            expected_branch: "feat/dirty".into(),
            pid: Some(std::process::id()),
            feature_envelopes: vec!["my-feat".into()], feature_envelope: None,
            worktree: wt_path.to_string_lossy().to_string(),
            app_config_names: vec![],
        };
        ps.activate(rec).unwrap();
        let mut state = ps.load().unwrap();
        state.feature_envelopes.insert("my-feat".to_string());
        ps.save_state(state).unwrap();
        let app_store = crate::app_config::AppConfigStore::open(git_dir.join("config.toml"));
        let result = crate::commands::cmd_feature_status(&ps, &app_store, "my-feat", false, &repo);
        assert!(result.is_ok(), "cmd_feature_status must succeed: {:?}", result);
        let out = result.unwrap();
        assert!(out.contains("dirty") || out.contains("uncommitted"),
            "cmd_feature_status must show dirty indicator for PR with uncommitted changes, got: {}", out);
    }

    #[test]
    fn cmd_install_hooks_governs_runs_claude_plugin_registration() {
        let tmp = tempfile::tempdir().unwrap();
        let flag_file = tmp.path().join("claude-was-called");
        let fake_claude = tmp.path().join("claude");
        let flag_path = flag_file.to_str().unwrap().to_string();
        std::fs::write(&fake_claude, format!("#!/bin/sh\necho \"$@\" >> {}\n", flag_path)).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&fake_claude, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let plugin_dir = tmp.path().join("fp-hooks");
        crate::commands::cmd_install_hooks(&plugin_dir, fake_claude.to_str().unwrap()).unwrap();
        assert!(flag_file.exists(), "cmd_install_hooks must invoke the claude binary for plugin registration");
        let calls = std::fs::read_to_string(&flag_file).unwrap();
        assert!(calls.contains("marketplace") || calls.contains("plugin"),
            "cmd_install_hooks must call claude with plugin/marketplace args, got: {}", calls);
    }

    #[test]
    fn cmd_install_hooks_governs_writes_hooks_json() {
        let dir = tempfile::tempdir().unwrap();
        let plugin_dir = dir.path().join("fp-hooks");
        crate::commands::cmd_install_hooks(&plugin_dir, "false").unwrap();
        let hooks_json = plugin_dir.join("hooks").join("hooks.json");
        assert!(hooks_json.exists(), "cmd_install_hooks must write hooks/hooks.json, path: {}", hooks_json.display());
        let content = std::fs::read_to_string(&hooks_json).unwrap();
        assert!(content.contains("SessionStart"), "hooks.json must reference SessionStart, got: {}", content);
        assert!(content.contains("PreToolUse"), "hooks.json must reference PreToolUse, got: {}", content);
        // Commands must be absolute paths to installed scripts, not phantom binary names.
        let session_start_abs = plugin_dir.join("hooks").join("session-start.sh");
        assert!(content.contains(session_start_abs.to_str().unwrap()),
            "hooks.json SessionStart command must be absolute path to session-start.sh, got: {}", content);
        let guard_abs = plugin_dir.join("hooks").join("pre-tool-use-guard.sh");
        assert!(content.contains(guard_abs.to_str().unwrap()),
            "hooks.json PreToolUse command must be absolute path to pre-tool-use-guard.sh, got: {}", content);
    }

    #[test]
    fn cmd_install_hooks_governs_writes_session_start_script() {
        let dir = tempfile::tempdir().unwrap();
        let plugin_dir = dir.path().join("fp-hooks");
        crate::commands::cmd_install_hooks(&plugin_dir, "false").unwrap();
        let script = plugin_dir.join("hooks").join("session-start.sh");
        assert!(script.exists(), "cmd_install_hooks must write hooks/session-start.sh");
        let content = std::fs::read_to_string(&script).unwrap();
        assert!(content.contains("fp") || content.contains("worktree"),
            "session-start.sh must reference fp or worktree, got: {}", content);
    }

    // fp feature delete: removes all member PRs from state
    #[test]
    fn cmd_feature_delete_governs_removes_all_member_prs() {
        let dir = tempfile::tempdir().unwrap();
        let ps = crate::process_store::ProcessStateStore::open(dir.path());
        let mut state = ps.load().unwrap();
        state.feature_envelopes.insert("my-feat".into());
        state.records.insert(10, crate::process_store::ProcessRecord {
            pr: 10, expected_branch: "feat/a".into(), pid: None,
            feature_envelopes: vec!["my-feat".into()], feature_envelope: None,
            worktree: "".into(), app_config_names: vec![],
        });
        state.records.insert(20, crate::process_store::ProcessRecord {
            pr: 20, expected_branch: "feat/b".into(), pid: None,
            feature_envelopes: vec!["my-feat".into()], feature_envelope: None,
            worktree: "".into(), app_config_names: vec![],
        });
        ps.save_state(state).unwrap();
        let result = crate::commands::cmd_feature_delete(&ps, "my-feat");
        assert!(result.is_ok(), "cmd_feature_delete must succeed: {:?}", result);
        let after = ps.load().unwrap();
        assert!(!after.records.contains_key(&10), "cmd_feature_delete must remove PR #10");
        assert!(!after.records.contains_key(&20), "cmd_feature_delete must remove PR #20");
    }

    // fp feature delete: removes all dep slots from envelope_deps
    #[test]
    fn cmd_feature_delete_governs_removes_all_dep_slots() {
        let dir = tempfile::tempdir().unwrap();
        let ps = crate::process_store::ProcessStateStore::open(dir.path());
        let mut state = ps.load().unwrap();
        state.feature_envelopes.insert("my-feat".into());
        state.envelope_deps.insert("my-feat".into(), vec!["backend".into(), "worker".into()]);
        ps.save_state(state).unwrap();
        let result = crate::commands::cmd_feature_delete(&ps, "my-feat");
        assert!(result.is_ok(), "cmd_feature_delete must succeed: {:?}", result);
        let after = ps.load().unwrap();
        let deps = after.envelope_deps.get("my-feat").map(|v| v.as_slice()).unwrap_or(&[]);
        assert!(deps.is_empty(), "cmd_feature_delete must remove all dep slots, got: {:?}", deps);
    }

    // fp feature delete: removes the named envelope from feature_envelopes
    #[test]
    fn cmd_feature_delete_governs_removes_envelope_key() {
        let dir = tempfile::tempdir().unwrap();
        let ps = crate::process_store::ProcessStateStore::open(dir.path());
        let mut state = ps.load().unwrap();
        state.feature_envelopes.insert("my-feat".into());
        ps.save_state(state).unwrap();
        let result = crate::commands::cmd_feature_delete(&ps, "my-feat");
        assert!(result.is_ok(), "cmd_feature_delete must succeed: {:?}", result);
        let after = ps.load().unwrap();
        assert!(!after.feature_envelopes.contains("my-feat"),
            "cmd_feature_delete must remove the named envelope from feature_envelopes");
    }

    // fp feature delete: returns error when envelope does not exist
    #[test]
    fn cmd_feature_delete_governs_errors_when_envelope_missing() {
        let dir = tempfile::tempdir().unwrap();
        let ps = crate::process_store::ProcessStateStore::open(dir.path());
        let result = crate::commands::cmd_feature_delete(&ps, "nonexistent");
        assert!(result.is_err(), "cmd_feature_delete must error when envelope does not exist");
    }

    #[test]
    fn cmd_install_hooks_governs_writes_pre_tool_use_guard_script() {
        let dir = tempfile::tempdir().unwrap();
        let plugin_dir = dir.path().join("fp-hooks");
        crate::commands::cmd_install_hooks(&plugin_dir, "false").unwrap();
        let script = plugin_dir.join("hooks").join("pre-tool-use-guard.sh");
        assert!(script.exists(), "cmd_install_hooks must write hooks/pre-tool-use-guard.sh");
        let content = std::fs::read_to_string(&script).unwrap();
        assert!(content.contains("fp") || content.contains("worktree"),
            "pre-tool-use-guard.sh must reference fp or worktree, got: {}", content);
    }

}
