use crate::tasks;
use crate::store;

pub fn format_watch_initial_state(pr: u64, title: &str, task_list: &[tasks::Task], json: bool, lock: Option<&str>, prefix: &str) -> String {
    if json {
        return serde_json::to_string(&serde_json::json!({
            "pr": pr,
            "initial_tasks": task_list,
        })).unwrap_or_default();
    }
    let task_prefix = prefix.replace("└─ ", "   ");
    let lock_suffix = lock.map(|s| format!("  {}", s)).unwrap_or_default();
    if task_list.is_empty() {
        return format!("PR #{} {} — ready{}\n", pr, title, lock_suffix);
    }
    let mut out = format!("PR #{} {} — {} task(s){}\n", pr, title, task_list.len(), lock_suffix);
    for t in task_list {
        let flag = if t.blocking { "[blocking]" } else { "[waiting]" };
        out.push_str(&format!("{}  {} {:?}: {}\n", task_prefix, flag, t.task_type, t.description));
    }
    out
}

#[allow(clippy::too_many_arguments)]
pub fn format_pr_status_all_entry(prefix: &str, number: u64, title: &str, tasks: &[tasks::Task], lock: &str, health: Option<&str>, is_closed: bool, is_merged: bool, draft: bool) -> String {
    let health_str = health.map(|h| format!("  [{}]", h)).unwrap_or_default();
    let state_tag = if is_merged { "  [merged]" } else if is_closed { "  [closed]" } else { "" };
    let draft_tag = if draft { "  [draft]" } else { "" };
    if tasks.is_empty() {
        return format!("{}PR #{} {} — ready{}{}{}{}\n", prefix, number, title, lock, health_str, state_tag, draft_tag);
    }
    let task_prefix = prefix.replace("└─ ", "   ");
    let mut out = format!("{}PR #{} {} — {} task(s){}{}{}{}\n", prefix, number, title, tasks.len(), lock, health_str, state_tag, draft_tag);
    for t in tasks {
        let flag = if t.blocking { "[blocking]" } else { "[waiting]" };
        out.push_str(&format!("{}  {} {:?}: {}\n", task_prefix, flag, t.task_type, t.description));
    }
    out
}

pub fn format_watch_event_json(pr: u64, new: &[tasks::Task], resolved: &[tasks::Task]) -> String {
    serde_json::to_string(&serde_json::json!({
        "pr": pr,
        "new": new,
        "resolved": resolved,
    })).unwrap_or_default()
}

pub fn format_adopt_message(branch: &str) -> String {
    format!("Adopted {} — checked out main in main worktree, created fp worktree\n", branch)
}

pub fn format_new_worktree_output(wt_path: &std::path::Path, branch: &str) -> String {
    format!("Created worktree at {}\nuse: fps {}\n", wt_path.display(), branch)
}

pub fn repo_header(owner: &str, repo: &str) -> String {
    format!("{}/{}", owner, repo)
}

pub fn format_single_pr_status(pr: u64, tasks: &[tasks::Task], lock: Option<&str>) -> String {
    let lock_str = lock.map(|s| format!("  {}", s)).unwrap_or_default();
    if tasks.is_empty() {
        format!("PR #{} is ready.{}", pr, lock_str)
    } else {
        let mut lines = vec![format!("PR #{} — {} task(s):{}", pr, tasks.len(), lock_str)];
        for t in tasks {
            let flag = if t.blocking { "[blocking]" } else { "[waiting]" };
            lines.push(format!("  {} {:?}: {}", flag, t.task_type, t.description));
        }
        lines.join("\n")
    }
}

pub fn format_worktree_add_error(stderr: &str, _branch: &str, pr: u64) -> String {
    if let Some(path) = stderr.lines()
        .find(|l| l.contains("already used by worktree at"))
        .and_then(|l| l.split("worktree at '").nth(1))
        .and_then(|s| s.strip_suffix('\'').or_else(|| s.split('\'').next()))
    {
        format!(
            "branch already has a worktree at {} — to relocate: git worktree remove {} && fps {}",
            path, path, pr
        )
    } else {
        format!("git worktree add failed: {}", stderr.trim())
    }
}

pub fn format_conflict_hint(branch: &str, prs: &std::collections::HashMap<u64, store::PrCache>) -> String {
    if let Some(pr) = prs.values().find(|p| p.branch == branch) {
        format!("  Tip: fps {} to switch to its worktree", pr.number)
    } else {
        String::new()
    }
}

pub fn format_open_threads(pr: u64, threads: &[&crate::model::Thread], json: bool) -> String {
    if json {
        return serde_json::to_string_pretty(threads).unwrap_or_default();
    }
    if threads.is_empty() {
        return format!("No open threads on PR #{}.", pr);
    }
    let mut out = format!("PR #{} — {} open thread(s):\n", pr, threads.len());
    for t in threads {
        let loc = match (&t.file, t.line) {
            (Some(f), Some(l)) => format!(" {}:{}", f, l),
            _ => String::new(),
        };
        out.push_str(&format!("  #{} ({:?}){}\n    {}\n", t.id, t.state, loc, t.body));
    }
    out
}

pub fn fetch_open_threads(threads: &[crate::model::Thread]) -> Vec<&crate::model::Thread> {
    threads.iter()
        .filter(|t| matches!(t.state, crate::model::ThreadState::Open | crate::model::ThreadState::Stale))
        .collect()
}

pub fn format_resolved_threads(pr: u64, threads: &[crate::model::ResolvedThreadInfo], json: bool) -> String {
    if json {
        let arr: Vec<serde_json::Value> = threads.iter().map(|t| serde_json::json!({
            "body": t.body, "file": t.file, "line": t.line,
            "resolved_by": t.resolved_by, "created_at": t.created_at,
            "first_commit_after_opened": t.first_commit_after_opened,
        })).collect();
        return serde_json::to_string_pretty(&arr).unwrap_or_default();
    }
    if threads.is_empty() {
        return format!("No resolved threads on PR #{}.", pr);
    }
    let mut out = format!("PR #{} — {} resolved thread(s):\n", pr, threads.len());
    for t in threads {
        let loc = match (&t.file, t.line) {
            (Some(f), Some(l)) => format!("  {}:{}", f, l),
            _ => "  ".to_string(),
        };
        out.push_str(&format!("{}  resolved by: {}\n", loc, t.resolved_by.as_deref().unwrap_or("unknown")));
        if let Some(at) = &t.created_at { out.push_str(&format!("    opened: {}\n", at)); }
        if let Some(c) = &t.first_commit_after_opened { out.push_str(&format!("    first commit after: {}\n", c)); }
        out.push_str(&format!("    {}\n", t.body));
    }
    out
}

pub fn watch_notification_messages(pr: u64, new: &[crate::tasks::Task], resolved: &[crate::tasks::Task]) -> Vec<(String, String)> {
    let title = format!("fp: #{}", pr);
    let mut msgs = Vec::new();
    for t in resolved {
        match t.task_type {
            tasks::TaskType::FixCi => msgs.push((title.clone(), format!("CI passing: {}", t.context_hint))),
            tasks::TaskType::AwaitingReview => msgs.push((title.clone(), "PR approved".into())),
            _ => {}
        }
    }
    for t in new {
        match t.task_type {
            tasks::TaskType::RespondThread => msgs.push((title.clone(), format!("New review thread: {}", t.description))),
            tasks::TaskType::FixCi => msgs.push((title.clone(), format!("CI failing: {}", t.context_hint))),
            _ => {}
        }
    }
    msgs
}
