#[cfg(test)]
mod tests {
    // D1/D2: format_watch_initial_state with json=true returns JSON with pr and initial_tasks
    #[test]
    fn format_watch_initial_state_json_contains_pr_and_tasks() {
        use crate::tasks::{Task, TaskType};
        let tasks = vec![Task {
            pr: 5, task_type: TaskType::FixCi, blocking: true,
            description: "Fix ci/test".into(), context_hint: "ci/test".into(),
        }];
        let out = crate::format_watch_initial_state(5, "my PR", &tasks, true);
        let parsed: serde_json::Value = serde_json::from_str(&out)
            .expect("json=true must return valid JSON");
        assert_eq!(parsed["pr"], 5, "JSON must contain pr number, got: {}", out);
        assert!(parsed["initial_tasks"].is_array(), "JSON must contain initial_tasks array, got: {}", out);
    }

    // D3: format_watch_initial_state with json=false and no tasks returns ready line
    #[test]
    fn format_watch_initial_state_text_empty_tasks_shows_ready() {
        let out = crate::format_watch_initial_state(7, "cool feature", &[], false);
        assert!(out.contains("ready"), "empty tasks should show 'ready', got: {}", out);
        assert!(out.contains("cool feature"), "output should contain PR title, got: {}", out);
    }

    // D4: format_watch_initial_state with json=false and tasks shows task count
    #[test]
    fn format_watch_initial_state_text_with_tasks_shows_count() {
        use crate::tasks::{Task, TaskType};
        let tasks = vec![
            Task { pr: 3, task_type: TaskType::FixCi, blocking: true, description: "Fix ci/lint".into(), context_hint: "ci/lint".into() },
            Task { pr: 3, task_type: TaskType::AwaitingReview, blocking: false, description: "Waiting for approval".into(), context_hint: "approval".into() },
        ];
        let out = crate::format_watch_initial_state(3, "refactor", &tasks, false);
        assert!(out.contains("2 task"), "should show 2 tasks, got: {}", out);
    }

    // D5: format_watch_initial_state with json=false and tasks shows each task description
    #[test]
    fn format_watch_initial_state_text_with_tasks_shows_each_task() {
        use crate::tasks::{Task, TaskType};
        let tasks = vec![
            Task { pr: 3, task_type: TaskType::FixCi, blocking: true, description: "Fix ci/lint".into(), context_hint: "ci/lint".into() },
        ];
        let out = crate::format_watch_initial_state(3, "refactor", &tasks, false);
        assert!(out.contains("Fix ci/lint"), "output should contain task description, got: {}", out);
    }

    // ADR-002 #9 D2: format_watch_event_json returns valid JSON with new and resolved arrays
    #[test]
    fn format_watch_event_json_contains_new_and_resolved() {
        use crate::tasks::{Task, TaskType};
        let new_tasks = vec![Task {
            pr: 7, task_type: TaskType::FixCi, blocking: true,
            description: "Fix failing check: ci/test".into(),
            context_hint: "ci/test".into(),
        }];
        let resolved: Vec<Task> = vec![];
        let json_str = crate::format_watch_event_json(7, &new_tasks, &resolved);
        let parsed: serde_json::Value = serde_json::from_str(&json_str)
            .expect("format_watch_event_json must return valid JSON");
        assert!(parsed["new"].is_array(), "JSON must contain 'new' array, got: {}", json_str);
        assert!(parsed["resolved"].is_array(), "JSON must contain 'resolved' array, got: {}", json_str);
    }

    // ADR-002 #9 D2: format_watch_event_json includes pr number
    #[test]
    fn format_watch_event_json_includes_pr_number() {
        use crate::tasks::{Task, TaskType};
        let new_tasks = vec![Task {
            pr: 42, task_type: TaskType::AwaitingCi, blocking: false,
            description: "Waiting for check: ci/lint".into(),
            context_hint: "ci/lint".into(),
        }];
        let json_str = crate::format_watch_event_json(42, &new_tasks, &[]);
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed["pr"], 42, "JSON must include pr number, got: {}", json_str);
    }

    // ADR-002 #9 D3: agent_context_manifest includes tracked_prs field
    #[test]
    fn agent_context_manifest_includes_tracked_prs() {
        use crate::store::TrackedPr;
        let prs = vec![
            TrackedPr { number: 1, title: "feat: foo".into(), branch: "feat/foo".into(), base: "main".into() },
        ];
        let manifest = crate::github::agent_context_manifest_with_prs(&prs);
        assert!(manifest["tracked_prs"].is_array(),
            "manifest must include tracked_prs array, got: {}", manifest);
        assert_eq!(manifest["tracked_prs"][0]["number"], 1,
            "tracked_prs must include PR number, got: {}", manifest);
    }

    // ADR-002 #9 D4: is_wait_condition_met returns true for ci-pass when no FixCi or AwaitingCi tasks
    #[test]
    fn is_wait_condition_met_ci_pass_when_no_ci_tasks() {
        use crate::tasks::{Task, TaskType};
        let tasks: Vec<Task> = vec![];
        assert!(crate::is_wait_condition_met("ci-pass", &tasks),
            "ci-pass should be met when no CI tasks present");
    }

    // ADR-002 #9 D4: is_wait_condition_met returns false for ci-pass when AwaitingCi present
    #[test]
    fn is_wait_condition_met_ci_pass_false_when_awaiting_ci() {
        use crate::tasks::{Task, TaskType};
        let tasks = vec![Task {
            pr: 1, task_type: TaskType::AwaitingCi, blocking: false,
            description: "Waiting for ci/test".into(), context_hint: "ci/test".into(),
        }];
        assert!(!crate::is_wait_condition_met("ci-pass", &tasks),
            "ci-pass should not be met when AwaitingCi task present");
    }

    // ADR-002 #9 D4: is_wait_condition_met returns false for ci-pass when FixCi present
    #[test]
    fn is_wait_condition_met_ci_pass_false_when_fix_ci() {
        use crate::tasks::{Task, TaskType};
        let tasks = vec![Task {
            pr: 1, task_type: TaskType::FixCi, blocking: true,
            description: "Fix failing check: ci/test".into(), context_hint: "ci/test".into(),
        }];
        assert!(!crate::is_wait_condition_met("ci-pass", &tasks),
            "ci-pass should not be met when FixCi task present");
    }

    // ADR-002 #9 D5: is_wait_condition_met returns true for ready when no blocking tasks
    #[test]
    fn is_wait_condition_met_ready_when_no_blocking_tasks() {
        use crate::tasks::{Task, TaskType};
        let tasks = vec![Task {
            pr: 1, task_type: TaskType::AwaitingCi, blocking: false,
            description: "Waiting for ci/test".into(), context_hint: "ci/test".into(),
        }];
        assert!(crate::is_wait_condition_met("ready", &tasks),
            "ready should be met when no blocking tasks");
    }

    // ADR-002 #9 D5: is_wait_condition_met returns false for ready when blocking task present
    #[test]
    fn is_wait_condition_met_ready_false_when_blocking() {
        use crate::tasks::{Task, TaskType};
        let tasks = vec![Task {
            pr: 1, task_type: TaskType::FixCi, blocking: true,
            description: "Fix failing check: ci/test".into(), context_hint: "ci/test".into(),
        }];
        assert!(!crate::is_wait_condition_met("ready", &tasks),
            "ready should not be met when blocking task present");
    }

    // ADR-002 #9 D6: save_profile writes a file containing github_token and owner/repo
    #[test]
    fn save_profile_writes_file_with_token_and_repo() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profiles.json");
        crate::profile::save_profile(&path, "work", "ghp_test_token", "myorg/myrepo")
            .expect("save_profile should succeed");
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("ghp_test_token"), "profile file should contain token");
        assert!(contents.contains("myorg/myrepo"), "profile file should contain repo");
    }

    // ADR-002 #9 D7: load_profile returns the token and repo saved under that name
    #[test]
    fn load_profile_returns_saved_token_and_repo() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profiles.json");
        crate::profile::save_profile(&path, "work", "ghp_my_token", "org/repo").unwrap();
        let p = crate::profile::load_profile(&path, "work")
            .expect("load_profile should succeed");
        assert_eq!(p.github_token, "ghp_my_token");
        assert_eq!(p.repo, "org/repo");
    }

    // ADR-002 #9 D7: load_profile returns Err for unknown profile name
    #[test]
    fn load_profile_errors_for_unknown_name() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profiles.json");
        let result = crate::profile::load_profile(&path, "nonexistent");
        assert!(result.is_err(), "load_profile for unknown name should return Err");
    }
}
