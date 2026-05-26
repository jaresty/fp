// Task generator — stub only

use crate::model::{CheckStatus, CownershipEligibility, PrState, ThreadState};

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    FixCi,
    RespondThread,
    AwaitingReview,
    AwaitingCi,
    MarkReady,
    MergeConflict,
    ReadyUnverified,
    RebaseOnParent,
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
            CheckStatus::Fail => {
                // Parse ESLint-format lines from log_snippet if present
                let eslint_tasks = check.log_snippet.as_deref()
                    .map(|log| parse_eslint_tasks(pr.number, log, check.required))
                    .unwrap_or_default();
                if eslint_tasks.is_empty() {
                    tasks.push(Task {
                        pr: pr.number,
                        task_type: TaskType::FixCi,
                        blocking: check.required,
                        description: format!("Fix failing check: {}", check.name),
                        context_hint: check.name.clone(),
                    });
                } else {
                    tasks.extend(eslint_tasks);
                }
            }
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

    // Merge conflict → blocking task
    if pr.has_merge_conflict {
        tasks.push(Task {
            pr: pr.number,
            task_type: TaskType::MergeConflict,
            blocking: true,
            description: "Resolve merge conflict".into(),
            context_hint: "merge_conflict".into(),
        });
    }

    // Parent PR ahead of child's base → child needs rebase
    if pr.needs_parent_rebase {
        tasks.push(Task {
            pr: pr.number,
            task_type: TaskType::RebaseOnParent,
            blocking: true,
            description: format!("Run `fp rebase-stack {}` — parent PR has new commits", pr.number),
            context_hint: "rebase_on_parent".into(),
        });
    }

    // CODEOWNERS eligibility check
    match &pr.codeowners_eligibility {
        CownershipEligibility::Unverifiable { reviewer } => tasks.push(Task {
            pr: pr.number,
            task_type: TaskType::ReadyUnverified,
            blocking: false,
            description: format!(
                "fp cannot verify CODEOWNERS eligibility for this approval. \
                 Confirm that {} is a required reviewer for the changed files before merging.",
                reviewer
            ),
            context_hint: "codeowners_unverified".into(),
        }),
        CownershipEligibility::Ineligible { reviewer: _, required_team } => tasks.push(Task {
            pr: pr.number,
            task_type: TaskType::AwaitingReview,
            blocking: false,
            description: format!("Approver is not eligible under CODEOWNERS; required team: {}", required_team),
            context_hint: "codeowners_ineligible".into(),
        }),
        CownershipEligibility::Verified => {}
    }

    // Draft PR with all checks green and no open threads → suggest marking ready
    if pr.draft
        && pr.checks.iter().all(|c| c.status == CheckStatus::Pass)
        && !pr.threads.iter().any(|t| matches!(t.state, ThreadState::Open | ThreadState::Stale))
        && !pr.is_stacked
    {
        tasks.push(Task {
            pr: pr.number,
            task_type: TaskType::MarkReady,
            blocking: false,
            description: format!("Run `fp ready {}` to mark this PR ready for review", pr.number),
            context_hint: "mark_ready".into(),
        });
    }

    tasks
}

/// Parse ESLint-format error lines from a log snippet.
/// ESLint format: `<file>:<line>:<col>  error  <message>  <rule>`
/// Returns one FixCi task per distinct ESLint error line found.
fn parse_eslint_tasks(pr: u64, log: &str, blocking: bool) -> Vec<Task> {
    let mut tasks = Vec::new();
    for line in log.lines() {
        // Detect lines matching: path:line:col  error  message  rule
        // Minimum: contains "  error  " and a rule identifier at the end
        if !line.contains("  error  ") { continue; }
        let parts: Vec<&str> = line.splitn(2, "  error  ").collect();
        if parts.len() < 2 { continue; }
        let after_error = parts[1];
        // Split message  rule — last whitespace-separated token is the rule
        let tokens: Vec<&str> = after_error.split_whitespace().collect();
        if tokens.is_empty() { continue; }
        let rule = tokens.last().unwrap().to_string();
        let message = tokens[..tokens.len().saturating_sub(1)].join(" ");
        let location = parts[0].trim().to_string();
        tasks.push(Task {
            pr,
            task_type: TaskType::FixCi,
            blocking,
            description: format!("ESLint {}: {} ({})", rule, message, location),
            context_hint: rule,
        });
    }
    tasks
}

pub fn is_wait_condition_met(condition: &str, task_list: &[Task]) -> bool {
    match condition {
        "ci-pass" => !task_list.iter().any(|t| matches!(
            t.task_type, TaskType::FixCi | TaskType::AwaitingCi
        )),
        "ready" => !task_list.iter().any(|t| t.blocking),
        _ => false,
    }
}
