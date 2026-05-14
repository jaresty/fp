#[cfg(test)]
mod tests {
    // ADR-002: watch_notification_messages returns CI-passing message when FixCi resolved
    #[test]
    fn watch_notifications_resolved_fixci_produces_ci_passing_message() {
        use crate::tasks::{Task, TaskType};
        let resolved = vec![Task {
            pr: 7, task_type: TaskType::FixCi, blocking: true,
            description: "Fix failing check: ci/test".into(),
            context_hint: "ci/test".into(),
        }];
        let msgs = crate::watch_notification_messages(7, &[], &resolved);
        assert!(
            msgs.iter().any(|(_, m)| m.contains("passing") || m.contains("CI") || m.contains("ci/test")),
            "expected CI-passing message for resolved FixCi, got: {:?}", msgs
        );
    }

    // ADR-002: watch_notification_messages returns approved message when AwaitingReview resolved
    #[test]
    fn watch_notifications_resolved_awaiting_review_produces_approved_message() {
        use crate::tasks::{Task, TaskType};
        let resolved = vec![Task {
            pr: 3, task_type: TaskType::AwaitingReview, blocking: false,
            description: "Waiting for approval".into(),
            context_hint: "approval".into(),
        }];
        let msgs = crate::watch_notification_messages(3, &[], &resolved);
        assert!(
            msgs.iter().any(|(_, m)| m.contains("approved") || m.contains("Approved")),
            "expected approved message for resolved AwaitingReview, got: {:?}", msgs
        );
    }

    // ADR-002: watch_notification_messages returns thread message when RespondThread appears
    #[test]
    fn watch_notifications_new_respond_thread_produces_thread_message() {
        use crate::tasks::{Task, TaskType};
        let new_tasks = vec![Task {
            pr: 5, task_type: TaskType::RespondThread, blocking: true,
            description: "Respond to thread #42".into(),
            context_hint: "thread:42".into(),
        }];
        let msgs = crate::watch_notification_messages(5, &new_tasks, &[]);
        assert!(
            msgs.iter().any(|(_, m)| m.contains("thread") || m.contains("Thread") || m.contains("review")),
            "expected thread message for new RespondThread, got: {:?}", msgs
        );
    }
}
