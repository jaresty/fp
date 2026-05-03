// Task generator — stub only

use crate::model::{PrState, CheckStatus, ThreadState};

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
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

/// Returns ordered tasks blocking readiness for a PR.
/// Empty vec means the PR is ready to merge.
pub fn generate_tasks(pr: &PrState) -> Vec<Task> {
    let mut tasks = Vec::new();

    // Required failing checks → fix_ci (blocking)
    for check in &pr.checks {
        if check.required {
            match check.status {
                CheckStatus::Fail => tasks.push(Task {
                    pr: pr.number,
                    task_type: TaskType::FixCi,
                    blocking: true,
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
