#[cfg(test)]
mod tests {
    use crate::model::*;
    use crate::tasks::{generate_tasks, TaskType};

    fn pr_clean() -> PrState {
        PrState {
            number: 1,
            title: "clean PR".into(),
            branch: "fix/foo".into(),
            draft: false,
            approved: true,
            checks: vec![Check {
                name: "ci/test".into(),
                status: CheckStatus::Pass,
                required: true,
                details_url: None,
            }],
            threads: vec![],
        }
    }

    // D4: empty task list when PR is fully ready
    #[test]
    fn ready_pr_has_no_tasks() {
        let pr = pr_clean();
        let tasks = generate_tasks(&pr);
        assert!(tasks.is_empty(), "expected no tasks, got: {:?}", tasks);
    }

    // D4: failing required check produces fix_ci task
    #[test]
    fn failing_required_check_produces_fix_ci_task() {
        let mut pr = pr_clean();
        pr.checks[0].status = CheckStatus::Fail;
        let tasks = generate_tasks(&pr);
        assert!(
            tasks.iter().any(|t| t.task_type == TaskType::FixCi),
            "expected fix_ci task, got: {:?}",
            tasks
        );
    }

    // D4: pending required check produces awaiting_ci (non-blocking)
    #[test]
    fn pending_required_check_produces_awaiting_ci() {
        let mut pr = pr_clean();
        pr.checks[0].status = CheckStatus::Pending;
        let tasks = generate_tasks(&pr);
        assert!(
            tasks.iter().any(|t| t.task_type == TaskType::AwaitingCi),
            "expected awaiting_ci task, got: {:?}",
            tasks
        );
    }

    // D4: open thread produces respond_thread task
    #[test]
    fn open_thread_produces_respond_thread_task() {
        let mut pr = pr_clean();
        pr.threads.push(Thread {
            id: 42,
            state: ThreadState::Open,
            body: "needs fix".into(),
            file: Some("src/main.rs".into()),
            line: Some(10),
        });
        let tasks = generate_tasks(&pr);
        assert!(
            tasks.iter().any(|t| t.task_type == TaskType::RespondThread),
            "expected respond_thread task, got: {:?}",
            tasks
        );
    }

    // D4: stale thread produces respond_thread task
    #[test]
    fn stale_thread_produces_respond_thread_task() {
        let mut pr = pr_clean();
        pr.threads.push(Thread {
            id: 43,
            state: ThreadState::Stale,
            body: "stale thread".into(),
            file: None,
            line: None,
        });
        let tasks = generate_tasks(&pr);
        assert!(
            tasks.iter().any(|t| t.task_type == TaskType::RespondThread),
            "expected respond_thread task for stale thread, got: {:?}",
            tasks
        );
    }

    // D4: resolved thread does not produce a task
    #[test]
    fn resolved_thread_produces_no_task() {
        let mut pr = pr_clean();
        pr.threads.push(Thread {
            id: 44,
            state: ThreadState::Resolved,
            body: "all good".into(),
            file: None,
            line: None,
        });
        let tasks = generate_tasks(&pr);
        assert!(
            !tasks.iter().any(|t| t.task_type == TaskType::RespondThread),
            "resolved thread should not produce task, got: {:?}",
            tasks
        );
    }

    // D4: no approval produces awaiting_review task
    #[test]
    fn no_approval_produces_awaiting_review_task() {
        let mut pr = pr_clean();
        pr.approved = false;
        let tasks = generate_tasks(&pr);
        assert!(
            tasks
                .iter()
                .any(|t| t.task_type == TaskType::AwaitingReview),
            "expected awaiting_review task, got: {:?}",
            tasks
        );
    }

    // D4: fix_ci is blocking; awaiting_ci is not blocking
    #[test]
    fn fix_ci_is_blocking_awaiting_ci_is_not() {
        let mut pr_fail = pr_clean();
        pr_fail.checks[0].status = CheckStatus::Fail;
        let fail_tasks = generate_tasks(&pr_fail);
        assert!(fail_tasks
            .iter()
            .any(|t| t.task_type == TaskType::FixCi && t.blocking));

        let mut pr_pending = pr_clean();
        pr_pending.checks[0].status = CheckStatus::Pending;
        let pending_tasks = generate_tasks(&pr_pending);
        assert!(pending_tasks
            .iter()
            .any(|t| t.task_type == TaskType::AwaitingCi && !t.blocking));
    }

    // D1: now covered by non_required_failing_check_produces_fix_ci_task
    // Keeping for historical clarity: optional failing check DOES produce FixCi (non-blocking)
    #[test]
    fn optional_failing_check_produces_fix_ci_non_blocking() {
        let mut pr = pr_clean();
        pr.checks.push(Check {
            name: "ci/optional".into(),
            status: CheckStatus::Fail,
            required: false,
            details_url: None,
        });
        let tasks = generate_tasks(&pr);
        assert!(
            tasks
                .iter()
                .any(|t| t.task_type == TaskType::FixCi && !t.blocking),
            "optional failing check should produce FixCi (non-blocking), got: {:?}",
            tasks
        );
    }

    // D1: non-required failing check produces FixCi task (blocking=false)
    #[test]
    fn non_required_failing_check_produces_fix_ci_task() {
        let mut pr = pr_clean();
        pr.checks[0].required = false;
        pr.checks[0].status = CheckStatus::Fail;
        let tasks = generate_tasks(&pr);
        assert!(
            tasks
                .iter()
                .any(|t| t.task_type == TaskType::FixCi && !t.blocking),
            "non-required failing check should produce FixCi (non-blocking), got: {:?}",
            tasks
        );
    }

    // D3: non-required pending check produces AwaitingCi task
    #[test]
    fn non_required_pending_check_produces_awaiting_ci() {
        let mut pr = pr_clean();
        pr.checks[0].required = false;
        pr.checks[0].status = CheckStatus::Pending;
        let tasks = generate_tasks(&pr);
        assert!(
            tasks
                .iter()
                .any(|t| t.task_type == TaskType::AwaitingCi && !t.blocking),
            "non-required pending check should produce AwaitingCi, got: {:?}",
            tasks
        );
    }

    // DW3: task_diff returns new tasks not in previous set
    #[test]
    fn task_diff_returns_new_tasks() {
        use crate::tasks::{task_diff, Task};
        let prev: Vec<Task> = vec![];
        let curr = vec![Task {
            pr: 1,
            task_type: TaskType::AwaitingReview,
            blocking: false,
            description: "Waiting for approval".into(),
            context_hint: "approval".into(),
        }];
        let (new, resolved) = task_diff(&prev, &curr);
        assert_eq!(new.len(), 1);
        assert!(resolved.is_empty());
    }

    // DW3: task_diff returns resolved tasks absent from current set
    #[test]
    fn task_diff_returns_resolved_tasks() {
        use crate::tasks::{task_diff, Task};
        let prev = vec![Task {
            pr: 1,
            task_type: TaskType::AwaitingReview,
            blocking: false,
            description: "Waiting for approval".into(),
            context_hint: "approval".into(),
        }];
        let curr: Vec<Task> = vec![];
        let (new, resolved) = task_diff(&prev, &curr);
        assert!(new.is_empty());
        assert_eq!(resolved.len(), 1);
    }

    // DW3: task_diff returns empty when tasks unchanged
    #[test]
    fn task_diff_returns_empty_when_unchanged() {
        use crate::tasks::{task_diff, Task};
        let tasks = vec![Task {
            pr: 1,
            task_type: TaskType::FixCi,
            blocking: true,
            description: "Fix failing check: ci/test".into(),
            context_hint: "ci/test".into(),
        }];
        let (new, resolved) = task_diff(&tasks, &tasks);
        assert!(new.is_empty());
        assert!(resolved.is_empty());
    }
}
