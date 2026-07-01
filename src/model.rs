// State model — types only, no logic

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Pass,
    Fail,
    Pending,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Check {
    pub name: String,
    pub status: CheckStatus,
    pub required: bool,
    #[serde(default)]
    pub details_url: Option<String>,
    #[serde(default)]
    pub log_snippet: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadState {
    Open,
    Addressed,
    Stale,
    Resolved,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Thread {
    pub id: u64,
    pub state: ThreadState,
    pub author: String,
    pub body: String,
    pub replies: Vec<(String, String)>,
    pub file: Option<String>,
    pub line: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum CownershipEligibility {
    #[default]
    Verified,
    Unverifiable { reviewer: String },
    Ineligible { reviewer: String, required_team: String },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PrState {
    pub number: u64,
    pub title: String,
    pub branch: String,
    pub base: String,
    #[serde(default)]
    pub head_sha: String,
    #[serde(default)]
    pub needs_parent_rebase: bool,
    pub draft: bool,
    pub approved: bool,
    pub checks: Vec<Check>,
    pub threads: Vec<Thread>,
    #[serde(default)]
    pub has_merge_conflict: bool,
    #[serde(default)]
    pub codeowners_eligibility: CownershipEligibility,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub is_stacked: bool,
    #[serde(default)]
    pub is_closed: bool,
    #[serde(default)]
    pub is_merged: bool,
}

#[derive(Debug, Clone)]
pub struct ResolvedThreadInfo {
    pub body: String,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub resolved_by: Option<String>,
    pub created_at: Option<String>,
    pub first_commit_after_opened: Option<String>,
}

pub fn parse_resolved_review_threads_from_graphql(json: &str) -> anyhow::Result<Vec<ResolvedThreadInfo>> {
    let v: serde_json::Value = serde_json::from_str(json)?;
    let pr = &v["data"]["repository"]["pullRequest"];
    let commits: Vec<(String, String, String)> = pr["commits"]["nodes"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .map(|n| {
            let c = &n["commit"];
            (
                c["committedDate"].as_str().unwrap_or("").to_string(),
                c["abbreviatedOid"].as_str().unwrap_or("").to_string(),
                c["messageHeadline"].as_str().unwrap_or("").to_string(),
            )
        })
        .collect();

    let mut result = vec![];
    for node in pr["reviewThreads"]["nodes"].as_array().unwrap_or(&vec![]) {
        if node["isResolved"].as_bool() != Some(true) { continue; }
        let resolved_by = node["resolvedBy"]["login"].as_str().map(|s| s.to_string());
        let first_comment = &node["comments"]["nodes"][0];
        let created_at = first_comment["createdAt"].as_str().map(|s| s.to_string());
        let body = first_comment["body"].as_str().unwrap_or("").to_string();
        let file = first_comment["path"].as_str().filter(|s| !s.is_empty()).map(|s| s.to_string());
        let line = first_comment["line"].as_u64().map(|n| n as u32);
        let first_commit_after_opened = created_at.as_deref().and_then(|opened_at| {
            commits.iter()
                .find(|(date, _, _)| date.as_str() > opened_at)
                .map(|(_, oid, msg)| format!("{} {}", oid, msg))
        });
        result.push(ResolvedThreadInfo { body, file, line, resolved_by, created_at, first_commit_after_opened });
    }
    Ok(result)
}
