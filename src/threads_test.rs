#[cfg(test)]
mod tests {
    #[test]
    fn display_governs_format_open_threads_empty() {
        let out = crate::display::format_open_threads(5, &[], false);
        assert!(
            out.contains("No open threads"),
            "display::format_open_threads empty must say 'No open threads': {}",
            out
        );
    }

    #[test]
    fn display_governs_fetch_open_threads_filters_resolved() {
        use crate::model::{Thread, ThreadState};
        let threads = vec![
            Thread { id: 1, body: "open".into(), state: ThreadState::Open, file: None, line: None, author: String::new(), replies: vec![] },
            Thread { id: 2, body: "resolved".into(), state: ThreadState::Resolved, file: None, line: None, author: String::new(), replies: vec![] },
        ];
        let open = crate::display::fetch_open_threads(&threads);
        assert_eq!(open.len(), 1, "display::fetch_open_threads must exclude resolved threads");
    }

    #[test]
    fn display_governs_format_adopt_message_contains_adopted_and_branch() {
        let msg = crate::display::format_adopt_message("feat/my-branch");
        assert!(msg.contains("Adopted"), "must contain 'Adopted', got: {}", msg);
        assert!(msg.contains("feat/my-branch"), "must contain branch name, got: {}", msg);
    }

    #[test]
    fn display_governs_format_new_worktree_output_contains_path_and_fps_hint() {
        let path = std::path::Path::new("/projects/vivaa-worktrees/feature/foo");
        let out = crate::display::format_new_worktree_output(path, "feature/foo");
        assert!(out.contains("Created worktree at"), "must contain 'Created worktree at', got: {}", out);
        assert!(out.contains("/projects/vivaa-worktrees/feature/foo"), "must contain path, got: {}", out);
        assert!(out.contains("fps feature/foo"), "must contain fps hint, got: {}", out);
    }

    #[test]
    fn display_governs_repo_header_contains_owner_slash_repo() {
        let h = crate::display::repo_header("alice", "myproject");
        assert!(h.contains("alice/myproject"), "repo_header must contain owner/repo, got: {}", h);
    }

    fn make_pr(number: u64, branch: &str, base: &str) -> crate::store::PrCache {
        crate::store::PrCache { number, title: format!("PR {}", number), branch: branch.into(), base: base.into() }
    }

    #[test]
    fn display_governs_format_pr_status_all_entry_shows_tasks_inline() {
        let tasks = vec![
            crate::tasks::Task { pr: 7, task_type: crate::tasks::TaskType::FixCi, description: "Fix ci/test".into(), blocking: true, context_hint: "".into() },
            crate::tasks::Task { pr: 7, task_type: crate::tasks::TaskType::AwaitingReview, description: "Waiting for review".into(), blocking: false, context_hint: "".into() },
        ];
        let out = crate::display::format_pr_status_all_entry("", 7, "My PR", &tasks, "");
        assert!(out.contains("PR #7"), "must contain PR number, got: {}", out);
        assert!(out.contains("[blocking]"), "must show blocking flag, got: {}", out);
        assert!(out.contains("Fix ci/test"), "must show task description, got: {}", out);
        assert!(out.contains("[waiting]"), "must show waiting flag, got: {}", out);
    }

    #[test]
    fn display_governs_format_pr_status_all_entry_shows_ready_when_no_tasks() {
        let out = crate::display::format_pr_status_all_entry("", 3, "Clean PR", &[], "");
        assert!(out.contains("ready"), "must say ready when no tasks, got: {}", out);
        assert!(!out.contains("[blocking]"), "must not show blocking when no tasks, got: {}", out);
    }

    #[test]
    fn display_governs_format_pr_status_all_entry_respects_prefix() {
        let tasks = vec![crate::tasks::Task { pr: 4, task_type: crate::tasks::TaskType::FixCi, description: "fix".into(), blocking: true, context_hint: "".into() }];
        let out = crate::display::format_pr_status_all_entry("  └─ ", 4, "Child PR", &tasks, "");
        assert!(out.starts_with("  └─ "), "must start with prefix, got: {}", out);
    }

    #[test]
    fn display_governs_stack_tree_order_child_has_indent_prefix() {
        let root = make_pr(1, "feature-a", "main");
        let child = make_pr(2, "feature-b", "feature-a");
        let prs = vec![&root, &child];
        let result = crate::stack::stack_tree_order(&prs);
        assert_eq!(result.len(), 2);
        let child_entry = result.iter().find(|(n, _)| *n == 2).unwrap();
        assert!(child_entry.1.contains("└─"), "child PR must contain └─, got: {:?}", child_entry.1);
    }

    #[test]
    fn display_governs_stack_tree_order_child_follows_parent() {
        let root = make_pr(1, "feature-a", "main");
        let child = make_pr(2, "feature-b", "feature-a");
        let prs = vec![&child, &root];
        let result = crate::stack::stack_tree_order(&prs);
        let root_pos = result.iter().position(|(n, _)| *n == 1).unwrap();
        let child_pos = result.iter().position(|(n, _)| *n == 2).unwrap();
        assert!(root_pos < child_pos, "root must appear before its child in output");
    }

    #[test]
    fn model_governs_parse_resolved_threads_returns_body() {
        let json = r#"{"data":{"repository":{"pullRequest":{"commits":{"nodes":[]},"reviewThreads":{"nodes":[{"isResolved":true,"resolvedBy":{"login":"alice"},"comments":{"nodes":[{"body":"fix this","path":"src/main.rs","line":10,"createdAt":"2024-01-01T00:00:00Z"}]}}]}}}}}"#;
        let threads = crate::model::parse_resolved_review_threads_from_graphql(json).unwrap();
        assert_eq!(threads.len(), 1, "model::parse_resolved_review_threads_from_graphql must parse one resolved thread");
        assert_eq!(threads[0].body, "fix this");
    }
}
