use crate::store::PrCache;

pub fn agent_context_manifest() -> serde_json::Value {
    serde_json::json!({
        "name": "fp",
        "version": env!("CARGO_PKG_VERSION"),
        "description": "PR convergence loop — surfaces blocking tasks, manages CI, rebases stacks",
        "auth_required": "GITHUB_TOKEN env var or gh CLI (gh auth login)",
        "commands": [
            {"name": "ls", "description": "List tracked PRs with status", "json": true},
            {"name": "status", "description": "Show blocking tasks for a PR", "json": true},
            {"name": "track", "description": "Add a PR to tracking", "flags": ["--branch", "--title"]},
            {"name": "untrack", "description": "Remove a PR from tracking"},
            {"name": "watch", "description": "Poll for task changes continuously", "flags": ["--once", "--interval"]},
            {"name": "reply", "description": "Post a reply to a review thread"},
            {"name": "context", "description": "Fetch CI logs or thread body for a task hint", "flags": ["--full-log"]},
            {"name": "threads", "description": "List review threads", "flags": ["--resolved", "--json"]},
            {"name": "create", "description": "Create a draft PR and track it"},
            {"name": "rebase-stack", "description": "Rebase all tracked PRs in stack order"},
            {"name": "agent-context", "description": "This manifest", "json": true}
        ]
    })
}

pub fn agent_context_manifest_with_prs(prs: &[PrCache]) -> serde_json::Value {
    let mut manifest = agent_context_manifest();
    manifest["tracked_prs"] = serde_json::json!(prs
        .iter()
        .map(|p| serde_json::json!({"number": p.number, "title": p.title, "branch": p.branch}))
        .collect::<Vec<_>>());
    manifest
}
