#[cfg(test)]
mod tests {
    #[test]
    fn display_governs_watch_notification_messages_ci_fail() {
        use crate::tasks::{Task, TaskType};
        let new = vec![Task {
            pr: 42,
            task_type: TaskType::FixCi,
            blocking: true,
            description: "build".into(),
            context_hint: "lint failed".into(),
        }];
        let msgs = crate::display::watch_notification_messages(42, &new, &[]);
        assert!(!msgs.is_empty(), "display::watch_notification_messages must produce message for FixCi new task");
        assert!(msgs[0].1.contains("CI failing"), "message must say 'CI failing': {}", msgs[0].1);
    }
}
