// Task generator — stub only

use crate::model::{CheckStatus, PrState, ThreadState};

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    FixCi,
    RespondThread,
    AwaitingReview,
    AwaitingCi,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Task {
    pub pr: u64,
    pub task_type: TaskType,
    pub blocking: bool,
    pub description: String,
    pub context_hint: String,
}

/// Returns (new_tasks, resolved_tasks) by comparing prev and curr task sets.
/// Identity is (pr, task_type, context_hint).
pub fn task_diff(prev: &[Task], curr: &[Task]) -> (Vec<Task>, Vec<Task>) {
    let key = |t: &Task| (t.pr, t.task_type.clone(), t.context_hint.clone());
    let prev_keys: std::collections::HashSet<_> = prev.iter().map(key).collect();
    let curr_keys: std::collections::HashSet<_> = curr.iter().map(key).collect();
    let new = curr
        .iter()
        .filter(|t| !prev_keys.contains(&key(t)))
        .cloned()
        .collect();
    let resolved = prev
        .iter()
        .filter(|t| !curr_keys.contains(&key(t)))
        .cloned()
        .collect();
    (new, resolved)
}

/// Returns ordered tasks blocking readiness for a PR.
/// Empty vec means the PR is ready to merge.
pub fn generate_tasks(pr: &PrState) -> Vec<Task> {
    let mut tasks = Vec::new();

    // All checks → produce tasks based on status, required gates blocking only
    for check in &pr.checks {
        match check.status {
            CheckStatus::Fail => tasks.push(Task {
                pr: pr.number,
                task_type: TaskType::FixCi,
                blocking: check.required,
                description: format!("Fix failing check: {}", check.name),
                context_hint: check.name.clone(),
            }),
            CheckStatus::Pending => tasks.push(Task {
                pr: pr.number,
                task_type: TaskType::AwaitingCi,
                blocking: false,
                description: format!("Waiting for check: {}", check.name),
                context_hint: check.name.clone(),
            }),
            CheckStatus::Pass => {}
        }
    }

    // Open or stale threads → respond_thread (blocking)
    for thread in &pr.threads {
        match thread.state {
            ThreadState::Open | ThreadState::Stale => tasks.push(Task {
                pr: pr.number,
                task_type: TaskType::RespondThread,
                blocking: true,
                description: format!("Respond to thread #{}: {}", thread.id, thread.body),
                context_hint: format!("thread:{}", thread.id),
            }),
            ThreadState::Addressed | ThreadState::Resolved => {}
        }
    }

    // No approval → awaiting_review (non-blocking)
    if !pr.approved {
        tasks.push(Task {
            pr: pr.number,
            task_type: TaskType::AwaitingReview,
            blocking: false,
            description: "Waiting for approval".into(),
            context_hint: "approval".into(),
        });
    }

    tasks
}
