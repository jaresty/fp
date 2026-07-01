#[cfg(test)]
mod tests {
    use crate::model::*;
    use crate::tasks::{generate_tasks, TaskType};

    fn pr_clean() -> PrState {
        PrState {
            number: 1,
            title: "clean PR".into(),
            branch: "fix/foo".into(),
            base: "main".into(),
            draft: false,
            approved: true,
            checks: vec![Check {
                name: "ci/test".into(),
                status: CheckStatus::Pass,
                required: true,
                details_url: None,
                log_snippet: None,
            }],
            threads: vec![],
            head_sha: "".into(),
            needs_parent_rebase: false,
            has_merge_conflict: false, codeowners_eligibility: Default::default(), created_at: None,
            is_stacked: false,
            is_closed: false,
            is_merged: false,
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
            author: "".into(),
            body: "needs fix".into(),
            replies: vec![],
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
            author: "".into(),
            body: "stale thread".into(),
            replies: vec![],
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
            author: "".into(),
            body: "all good".into(),
            replies: vec![],
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
            log_snippet: None,
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

    // MR1: draft PR with all passing checks and no open threads generates MarkReady task
    #[test]
    fn draft_pr_with_all_passing_checks_generates_mark_ready_task() {
        let pr = PrState {
            number: 10,
            title: "Draft feature".into(),
            branch: "feat/draft".into(),
            base: "main".into(),
            draft: true,
            approved: false,
            checks: vec![Check { name: "ci".into(), status: CheckStatus::Pass, required: true, details_url: None, log_snippet: None }],
            threads: vec![],
            head_sha: "".into(), needs_parent_rebase: false, has_merge_conflict: false, codeowners_eligibility: Default::default(), created_at: None,
            is_stacked: false,
            is_closed: false,
            is_merged: false,
        };
        let tasks = generate_tasks(&pr);
        assert!(
            tasks.iter().any(|t| t.task_type == TaskType::MarkReady),
            "expected MarkReady task for draft PR with all green checks, got: {:?}",
            tasks.iter().map(|t| &t.task_type).collect::<Vec<_>>()
        );
    }

    // MR2: non-draft PR does not generate MarkReady task
    #[test]
    fn non_draft_pr_does_not_generate_mark_ready_task() {
        let pr = PrState {
            number: 11,
            title: "Open feature".into(),
            branch: "feat/open".into(),
            base: "main".into(),
            draft: false,
            approved: false,
            checks: vec![Check { name: "ci".into(), status: CheckStatus::Pass, required: true, details_url: None, log_snippet: None }],
            threads: vec![],
            head_sha: "".into(), needs_parent_rebase: false,
            has_merge_conflict: false, codeowners_eligibility: Default::default(), created_at: None,
            is_stacked: false,
            is_closed: false,
            is_merged: false,
        };
        let tasks = generate_tasks(&pr);
        assert!(
            !tasks.iter().any(|t| t.task_type == TaskType::MarkReady),
            "expected no MarkReady task for non-draft PR"
        );
    }

    // MR3: stacked (child) draft PR does not generate MarkReady task
    #[test]
    fn stacked_pr_has_no_markready_task() {
        let pr = PrState {
            number: 12,
            title: "Stacked feature".into(),
            branch: "feat/child".into(),
            base: "feat/parent".into(),
            draft: true,
            approved: false,
            checks: vec![Check { name: "ci".into(), status: CheckStatus::Pass, required: true, details_url: None, log_snippet: None }],
            threads: vec![],
            head_sha: "".into(), needs_parent_rebase: false, has_merge_conflict: false, codeowners_eligibility: Default::default(), created_at: None,
            is_stacked: true, is_closed: false, is_merged: false,
        };
        let tasks = generate_tasks(&pr);
        assert!(
            !tasks.iter().any(|t| t.task_type == TaskType::MarkReady),
            "expected no MarkReady task for stacked draft PR, got: {:?}",
            tasks.iter().map(|t| &t.task_type).collect::<Vec<_>>()
        );
    }

    // ADR-006: ESLint errors in check log_snippet produce FixCi tasks
    #[test]
    fn eslint_errors_in_log_produce_fix_ci_tasks() {
        let mut pr = pr_clean();
        pr.checks[0].status = CheckStatus::Fail;
        pr.checks[0].log_snippet = Some(
            "src/foo.ts:10:5  error  'x' is not defined  no-undef\n\
             src/bar.ts:3:1  error  Missing semicolon  semi\n\
             ✖ 2 problems (2 errors, 0 warnings)".into()
        );
        let tasks = generate_tasks(&pr);
        let fix_ci: Vec<_> = tasks.iter().filter(|t| t.task_type == TaskType::FixCi).collect();
        assert!(
            fix_ci.iter().any(|t| t.context_hint.contains("no-undef") || t.description.contains("no-undef")),
            "expected FixCi task for no-undef ESLint error, got: {:?}", fix_ci
        );
        assert!(
            fix_ci.iter().any(|t| t.context_hint.contains("semi") || t.description.contains("semi")),
            "expected FixCi task for semi ESLint error, got: {:?}", fix_ci
        );
    }

    // ADR-006: merge conflict produces merge_conflict task (blocking)
    #[test]
    fn merge_conflict_produces_task() {
        let mut pr = pr_clean();
        pr.has_merge_conflict = true;
        let tasks = generate_tasks(&pr);
        assert!(
            tasks.iter().any(|t| t.task_type == TaskType::MergeConflict && t.blocking),
            "expected blocking MergeConflict task, got: {:?}", tasks
        );
    }

    // ADR-006: no merge conflict produces no merge_conflict task
    #[test]
    fn no_merge_conflict_produces_no_task() {
        let pr = pr_clean();
        let tasks = generate_tasks(&pr);
        assert!(
            !tasks.iter().any(|t| t.task_type == TaskType::MergeConflict),
            "expected no MergeConflict task when no conflict, got: {:?}", tasks
        );
    }

    // ADR-002 #8: unresolvable CODEOWNERS (Unverifiable) on approved PR produces ReadyUnverified task
    #[test]
    fn unverifiable_codeowners_produces_ready_unverified_task() {
        let mut pr = pr_clean();
        pr.codeowners_eligibility = CownershipEligibility::Unverifiable { reviewer: "alice".into() };
        let tasks = generate_tasks(&pr);
        assert!(
            tasks.iter().any(|t| t.task_type == TaskType::ReadyUnverified),
            "expected ReadyUnverified task when CODEOWNERS unverifiable, got: {:?}", tasks
        );
    }

    // ADR-002 #8: ReadyUnverified task description names the reviewer
    #[test]
    fn ready_unverified_description_names_reviewer() {
        let mut pr = pr_clean();
        pr.codeowners_eligibility = CownershipEligibility::Unverifiable { reviewer: "bob".into() };
        let tasks = generate_tasks(&pr);
        let task = tasks.iter().find(|t| t.task_type == TaskType::ReadyUnverified).unwrap();
        assert!(task.description.contains("bob"), "description should name reviewer, got: {}", task.description);
    }

    // ADR-002 #8: ineligible CODEOWNERS approver produces AwaitingReview naming the team
    #[test]
    fn ineligible_codeowners_approver_produces_awaiting_review_with_team() {
        let mut pr = pr_clean();
        pr.codeowners_eligibility = CownershipEligibility::Ineligible { reviewer: "carol".into(), required_team: "org/reviewers".into() };
        let tasks = generate_tasks(&pr);
        assert!(
            tasks.iter().any(|t| t.task_type == TaskType::AwaitingReview),
            "expected AwaitingReview when approver ineligible, got: {:?}", tasks
        );
        let task = tasks.iter().find(|t| t.task_type == TaskType::AwaitingReview).unwrap();
        assert!(task.description.contains("org/reviewers"), "AwaitingReview should name required team, got: {}", task.description);
    }

    // ADR-002 #8: eligible CODEOWNERS produces no ReadyUnverified task (existing ready path unchanged)
    #[test]
    fn eligible_codeowners_produces_no_ready_unverified_task() {
        let pr = pr_clean(); // codeowners_eligibility defaults to Verified
        let tasks = generate_tasks(&pr);
        assert!(
            !tasks.iter().any(|t| t.task_type == TaskType::ReadyUnverified),
            "Verified eligibility should not produce ReadyUnverified, got: {:?}", tasks
        );
        assert!(tasks.is_empty(), "clean PR with Verified eligibility should have no tasks, got: {:?}", tasks);
    }

    // RS: RebaseOnParent task when needs_parent_rebase is true
    #[test]
    fn rebase_on_parent_task_when_needs_parent_rebase_is_true() {
        let mut child = pr_clean();
        child.needs_parent_rebase = true;
        let tasks = generate_tasks(&child);
        assert!(
            tasks.iter().any(|t| t.task_type == TaskType::RebaseOnParent && t.blocking),
            "expected blocking RebaseOnParent task, got: {:?}", tasks
        );
    }

    // RS: no RebaseOnParent task when needs_parent_rebase is false
    #[test]
    fn no_rebase_on_parent_task_when_needs_parent_rebase_is_false() {
        let pr = pr_clean(); // default false
        let tasks = generate_tasks(&pr);
        assert!(
            !tasks.iter().any(|t| t.task_type == TaskType::RebaseOnParent),
            "expected no RebaseOnParent task when needs_parent_rebase is false, got: {:?}", tasks
        );
    }

    // RS: no RebaseOnParent task when no parent (kept for clarity)
    #[test]
    fn no_rebase_on_parent_task_when_no_parent() {
        let pr = pr_clean();
        let tasks = generate_tasks(&pr);
        assert!(
            !tasks.iter().any(|t| t.task_type == TaskType::RebaseOnParent),
            "expected no RebaseOnParent task when no parent, got: {:?}", tasks
        );
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
