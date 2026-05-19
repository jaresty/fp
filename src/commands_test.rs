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
}
