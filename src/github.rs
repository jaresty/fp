use anyhow::Result;
use std::collections::HashSet;
use crate::model::{Check, CheckStatus, PrState, Thread, ThreadState};

fn parse_next_link(link_header: &str) -> Option<String> {
    // Link: <url>; rel="next", <url>; rel="last"
    for part in link_header.split(',') {
        let part = part.trim();
        if part.contains(r#"rel="next""#) && let (Some(start), Some(end)) = (part.find('<'), part.find('>')) {
            return Some(part[start + 1..end].to_string());
        }
    }
    None
}

#[derive(Clone)]
pub struct GithubClient {
    token: String,
    base_url: String,
}

impl GithubClient {
    pub fn new(token: String) -> Self {
        GithubClient { token, base_url: "https://api.github.com".into() }
    }

    #[cfg(test)]
    pub fn with_base_url(token: String, base_url: String) -> Self {
        GithubClient { token, base_url }
    }

    /// Fetch multiple PRs in parallel and return a map keyed by PR number.
    pub fn fetch_prs_as_map(&self, owner: &str, repo: &str, pr_numbers: &[u64]) -> std::collections::HashMap<u64, crate::model::PrState> {
        self.fetch_prs_parallel(owner, repo, pr_numbers)
            .into_iter().map(|p| (p.number, p)).collect()
    }

    /// Fetch multiple PRs in parallel, capped at 8 concurrent requests.
    /// Returns successfully fetched PrStates (failures silently dropped).
    pub fn fetch_prs_parallel(&self, owner: &str, repo: &str, pr_numbers: &[u64]) -> Vec<crate::model::PrState> {
        use std::sync::{Arc, Mutex};
        const MAX_CONCURRENT: usize = 8;
        let semaphore = Arc::new(Mutex::new(MAX_CONCURRENT));
        let owner = owner.to_string();
        let repo = repo.to_string();

        let handles: Vec<_> = pr_numbers.iter().map(|&number| {
            let client = self.clone();
            let owner = owner.clone();
            let repo = repo.clone();
            let sem = Arc::clone(&semaphore);
            std::thread::spawn(move || {
                // Acquire slot
                loop {
                    let mut count = sem.lock().unwrap();
                    if *count > 0 {
                        *count -= 1;
                        break;
                    }
                    drop(count);
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                let result = client.fetch_pr(&owner, &repo, number).ok();
                // Release slot
                *sem.lock().unwrap() += 1;
                result
            })
        }).collect();

        handles.into_iter()
            .filter_map(|h| h.join().ok().flatten())
            .collect()
    }

    pub(crate) fn get(&self, path: &str) -> Result<serde_json::Value> {
        let url = format!("{}{}", self.base_url, path);
        let resp = reqwest::blocking::Client::new()
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "fp/0.1")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()?
            .error_for_status()?
            .json::<serde_json::Value>()?;
        Ok(resp)
    }

    fn get_paginated(&self, path: &str) -> Result<serde_json::Value> {
        let sep = if path.contains('?') { "&" } else { "?" };
        let first_url = format!("{}{}{sep}per_page=100&page=1", self.base_url, path);
        let mut all_items: Vec<serde_json::Value> = Vec::new();
        let mut next_url: Option<String> = Some(first_url);
        while let Some(url) = next_url {
            let resp = reqwest::blocking::Client::new()
                .get(&url)
                .header("Authorization", format!("Bearer {}", self.token))
                .header("Accept", "application/vnd.github+json")
                .header("User-Agent", "fp/0.1")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .send()?
                .error_for_status()?;
            next_url = resp.headers()
                .get("link")
                .and_then(|v| v.to_str().ok())
                .and_then(parse_next_link);
            let page: serde_json::Value = resp.json()?;
            if let Some(arr) = page.as_array() {
                all_items.extend(arr.iter().cloned());
            }
        }
        Ok(serde_json::Value::Array(all_items))
    }

    /// Route reply to the correct API based on thread type:
    /// - inline (file is Some) → pulls/comments/replies
    /// - PR-level (file is None) → issues/comments
    pub fn reply_to_thread(&self, owner: &str, repo: &str, pr_number: u64, thread: &crate::model::Thread, body: &str) -> Result<String> {
        if thread.file.is_some() {
            self.reply_to_comment(owner, repo, pr_number, thread.id, body)
        } else {
            let body_with_mention = format!("@{} {}", thread.author, body);
            self.post_pr_comment(owner, repo, pr_number, &body_with_mention)
        }
    }

    pub fn reply_to_comment(&self, owner: &str, repo: &str, pr_number: u64, comment_id: u64, body: &str) -> Result<String> {
        let url = format!("{}/repos/{}/{}/pulls/{}/comments/{}/replies", self.base_url, owner, repo, pr_number, comment_id);
        let payload = serde_json::json!({ "body": body });
        let resp = reqwest::blocking::Client::new()
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "fp/0.1")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .json(&payload)
            .send()?
            .error_for_status()?
            .json::<serde_json::Value>()?;
        Ok(resp["body"].as_str().unwrap_or("").to_string())
    }

    pub fn post_pr_comment(&self, owner: &str, repo: &str, pr_number: u64, body: &str) -> Result<String> {
        let url = format!("{}/repos/{}/{}/issues/{}/comments", self.base_url, owner, repo, pr_number);
        let payload = serde_json::json!({ "body": body });
        let resp = reqwest::blocking::Client::new()
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "fp/0.1")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .json(&payload)
            .send()?
            .error_for_status()?
            .json::<serde_json::Value>()?;
        Ok(resp["html_url"].as_str().unwrap_or("").to_string())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_pr_with_body(&self, owner: &str, repo: &str, title: &str, head: &str, base: &str, draft: bool, body: Option<&str>) -> Result<PrState> {
        let url = format!("{}/repos/{}/{}/pulls", self.base_url, owner, repo);
        let mut payload = serde_json::json!({ "title": title, "head": head, "base": base, "draft": draft });
        if let Some(b) = body {
            payload["body"] = serde_json::Value::String(b.to_string());
        }
        let resp = reqwest::blocking::Client::new()
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "fp/0.1")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .json(&payload)
            .send()?
            .error_for_status()?
            .json::<serde_json::Value>()?;
        Ok(PrState {
            number: resp["number"].as_u64().unwrap_or(0),
            title: resp["title"].as_str().unwrap_or("").to_string(),
            branch: resp["head"]["ref"].as_str().unwrap_or("").to_string(),
            base: resp["base"]["ref"].as_str().unwrap_or("").to_string(),
            head_sha: resp["head"]["sha"].as_str().unwrap_or("").to_string(),
            draft: resp["draft"].as_bool().unwrap_or(false),
            approved: false,
            checks: vec![],
            threads: vec![],
            needs_parent_rebase: false, has_merge_conflict: false, codeowners_eligibility: Default::default(),
        })
    }

    pub fn fetch_resolved_threads_graphql(&self, owner: &str, repo: &str, pr_number: u64) -> Result<Vec<ResolvedThreadInfo>> {
        let query = format!(
            r#"query {{
              repository(owner: "{owner}", name: "{repo}") {{
                pullRequest(number: {pr_number}) {{
                  reviewThreads(first: 100) {{
                    nodes {{
                      id isResolved
                      resolvedBy {{ login }}
                      comments(first: 1) {{
                        nodes {{ createdAt body path line }}
                      }}
                    }}
                  }}
                  commits(last: 50) {{
                    nodes {{
                      commit {{ abbreviatedOid committedDate messageHeadline }}
                    }}
                  }}
                }}
              }}
            }}"#
        );
        let url = format!("{}/graphql", self.base_url);
        let resp = reqwest::blocking::Client::new()
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "fp/0.1")
            .json(&serde_json::json!({ "query": query }))
            .send()?
            .error_for_status()?
            .text()?;
        parse_resolved_review_threads_from_graphql(&resp)
    }

    pub fn mark_pr_ready(&self, owner: &str, repo: &str, pr_number: u64) -> Result<()> {
        let pr = self.get(&format!("/repos/{}/{}/pulls/{}", owner, repo, pr_number))?;
        let node_id = pr["node_id"].as_str().ok_or_else(|| anyhow::anyhow!("missing node_id"))?;
        let query = format!(
            "mutation {{ markPullRequestReadyForReview(input: {{ pullRequestId: \"{}\" }}) {{ pullRequest {{ isDraft }} }} }}",
            node_id
        );
        let url = format!("{}/graphql", self.base_url);
        reqwest::blocking::Client::new()
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "fp/0.1")
            .json(&serde_json::json!({ "query": query }))
            .send()?
            .error_for_status()?;
        Ok(())
    }

    /// Returns true if `head_sha` is behind `base_sha` (base has commits not reachable from head).
    pub fn is_head_behind_base(&self, owner: &str, repo: &str, base_sha: &str, head_sha: &str) -> bool {
        self.get(&format!("/repos/{}/{}/compare/{}...{}", owner, repo, base_sha, head_sha))
            .ok()
            .and_then(|j| j["behind_by"].as_u64())
            .map(|n| n > 0)
            .unwrap_or(false)
    }

    pub fn fetch_pr_head_sha_and_base(&self, owner: &str, repo: &str, pr_number: u64) -> Result<(String, String)> {
        let resp = self.get(&format!("/repos/{}/{}/pulls/{}", owner, repo, pr_number))?;
        let sha = resp["head"]["sha"].as_str().unwrap_or("").to_string();
        let base = resp["base"]["ref"].as_str().unwrap_or("").to_string();
        Ok((sha, base))
    }

    pub fn fetch_pr_is_merged(&self, owner: &str, repo: &str, pr_number: u64) -> Result<bool> {
        let resp = self.get(&format!("/repos/{}/{}/pulls/{}", owner, repo, pr_number))?;
        Ok(resp["merged"].as_bool().unwrap_or(false))
    }

    pub fn fetch_repo_merge_method(&self, owner: &str, repo: &str) -> Result<String> {
        let body = self.get(&format!("/repos/{}/{}", owner, repo))?;
        let squash = body["allow_squash_merge"].as_bool().unwrap_or(false);
        let merge = body["allow_merge_commit"].as_bool().unwrap_or(false);
        let rebase = body["allow_rebase_merge"].as_bool().unwrap_or(false);
        if squash { return Ok("squash".to_string()); }
        if rebase { return Ok("rebase".to_string()); }
        if merge { return Ok("merge".to_string()); }
        Ok("squash".to_string())
    }

    pub fn merge_pr(&self, owner: &str, repo: &str, pr_number: u64, merge_method: Option<&str>) -> Result<String> {
        let url = format!("{}/repos/{}/{}/pulls/{}/merge", self.base_url, owner, repo, pr_number);
        let mut payload = serde_json::json!({});
        if let Some(method) = merge_method {
            payload["merge_method"] = serde_json::Value::String(method.to_string());
        }
        let resp = reqwest::blocking::Client::new()
            .put(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "fp/0.1")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .json(&payload)
            .send()?
            .error_for_status()?
            .json::<serde_json::Value>()?;
        Ok(resp["sha"].as_str().unwrap_or("").to_string())
    }

    pub fn update_pr(&self, owner: &str, repo: &str, pr_number: u64, title: Option<&str>, body: Option<&str>) -> Result<()> {
        let url = format!("{}/repos/{}/{}/pulls/{}", self.base_url, owner, repo, pr_number);
        let mut payload = serde_json::json!({});
        if let Some(t) = title { payload["title"] = serde_json::Value::String(t.to_string()); }
        if let Some(b) = body  { payload["body"]  = serde_json::Value::String(b.to_string()); }
        let resp = reqwest::blocking::Client::new()
            .patch(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "fp/0.1")
            .json(&payload)
            .send()?;
        anyhow::ensure!(resp.status().is_success(), "update_pr failed: {}", resp.status());
        Ok(())
    }

    pub fn update_pr_base(&self, owner: &str, repo: &str, pr_number: u64, new_base: &str) -> Result<()> {
        let url = format!("{}/repos/{}/{}/pulls/{}", self.base_url, owner, repo, pr_number);
        let payload = serde_json::json!({ "base": new_base });
        reqwest::blocking::Client::new()
            .patch(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "fp/0.1")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .json(&payload)
            .send()?
            .error_for_status()?;
        Ok(())
    }

    pub fn fetch_checks_for_sha(&self, owner: &str, repo: &str, sha: &str) -> Result<Vec<Check>> {
        let checks_json = self.get(&format!("/repos/{}/{}/commits/{}/check-runs?per_page=100&page=1", owner, repo, sha))?;
        let mut name_order: Vec<String> = Vec::new();
        let mut best_run_by_name: std::collections::HashMap<String, serde_json::Value> = std::collections::HashMap::new();
        for c in checks_json["check_runs"].as_array().unwrap_or(&vec![]) {
            let name = c["name"].as_str().unwrap_or("").to_string();
            let started_at = c["started_at"].as_str().unwrap_or("").to_string();
            if let Some(existing) = best_run_by_name.get(&name) {
                if started_at.as_str() > existing["started_at"].as_str().unwrap_or("") {
                    best_run_by_name.insert(name, c.clone());
                }
            } else {
                name_order.push(name.clone());
                best_run_by_name.insert(name, c.clone());
            }
        }
        Ok(name_order.iter()
            .filter_map(|name| best_run_by_name.get(name))
            .map(|c| {
                let name = c["name"].as_str().unwrap_or("").to_string();
                let status = match (c["status"].as_str(), c["conclusion"].as_str()) {
                    (_, Some("success")) | (_, Some("skipped")) | (_, Some("neutral")) => CheckStatus::Pass,
                    (_, Some("failure")) | (_, Some("timed_out")) | (_, Some("cancelled")) => CheckStatus::Fail,
                    (Some("completed"), _) => CheckStatus::Fail,
                    _ => CheckStatus::Pending,
                };
                let details_url = c["details_url"].as_str().map(String::from);
                Check { name, status, required: false, details_url, log_snippet: None }
            })
            .collect())
    }

    pub fn fetch_pr_base(&self, owner: &str, repo: &str, pr_number: u64) -> Result<String> {
        let url = format!("{}/repos/{}/{}/pulls/{}", self.base_url, owner, repo, pr_number);
        let resp = self.get(&url.replace(&self.base_url, ""))?;
        Ok(resp["base"]["ref"].as_str().unwrap_or("").to_string())
    }

    pub fn fetch_pr_body(&self, owner: &str, repo: &str, pr_number: u64) -> Result<String> {
        let pr_json = self.get(&format!("/repos/{}/{}/pulls/{}", owner, repo, pr_number))?;
        Ok(pr_json["body"].as_str().unwrap_or("").to_string())
    }

    pub fn fetch_pr_metadata(&self, owner: &str, repo: &str, pr_number: u64) -> Result<(String, String)> {
        let pr_json = self.get(&format!("/repos/{}/{}/pulls/{}", owner, repo, pr_number))?;
        let title = pr_json["title"].as_str().unwrap_or("").to_string();
        let branch = pr_json["head"]["ref"].as_str().unwrap_or("").to_string();
        Ok((title, branch))
    }

    pub fn fetch_pr(&self, owner: &str, repo: &str, pr_number: u64) -> Result<PrState> {
        // 1. PR metadata
        let pr_json = self.get(&format!("/repos/{}/{}/pulls/{}", owner, repo, pr_number))?;
        let title = pr_json["title"].as_str().unwrap_or("").to_string();
        let branch = pr_json["head"]["ref"].as_str().unwrap_or("").to_string();
        let head_sha = pr_json["head"]["sha"].as_str().unwrap_or("").to_string();
        let draft = pr_json["draft"].as_bool().unwrap_or(false);
        let base_branch = pr_json["base"]["ref"].as_str().unwrap_or("main").to_string();

        // 2. Check runs
        let encoded_branch = branch.replace('/', "%2F");
        let checks_json = self.get(&format!(
            "/repos/{}/{}/commits/{}/check-runs", owner, repo, encoded_branch
        ))?;

        // 3. Branch protection from BASE branch (feature branches have no protection rules)
        let encoded_base = base_branch.replace('/', "%2F");
        let required_names: HashSet<String> = self
            .get(&format!("/repos/{}/{}/branches/{}/protection", owner, repo, encoded_base))
            .ok()
            .and_then(|j| {
                j["required_status_checks"]["contexts"]
                    .as_array()
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            })
            .unwrap_or_default();

        // 3b. Commit statuses (e.g. Buildkite) — uses SHA, not branch name
        let sha = pr_json["head"]["sha"].as_str().unwrap_or("").to_string();
        let statuses_json = self.get(&format!(
            "/repos/{}/{}/commits/{}/statuses", owner, repo, sha
        )).unwrap_or_default();

        // Build check-runs, deduplicated by name keeping the latest started_at (ISO8601 lexicographic)
        // Insertion order preserved: track name ordering for stable output
        let mut name_order: Vec<String> = Vec::new();
        let mut best_run_by_name: std::collections::HashMap<String, serde_json::Value> = std::collections::HashMap::new();
        for c in checks_json["check_runs"].as_array().unwrap_or(&vec![]) {
            let name = c["name"].as_str().unwrap_or("").to_string();
            let started_at = c["started_at"].as_str().unwrap_or("").to_string();
            if let Some(existing) = best_run_by_name.get(&name) {
                if started_at.as_str() > existing["started_at"].as_str().unwrap_or("") {
                    best_run_by_name.insert(name, c.clone());
                }
            } else {
                name_order.push(name.clone());
                best_run_by_name.insert(name, c.clone());
            }
        }
        let mut checks: Vec<Check> = name_order.iter()
            .filter_map(|name| best_run_by_name.get(name))
            .map(|c| {
                let name = c["name"].as_str().unwrap_or("").to_string();
                let status = match (c["status"].as_str(), c["conclusion"].as_str()) {
                    (_, Some("success")) | (_, Some("skipped")) | (_, Some("neutral")) => CheckStatus::Pass,
                    (_, Some("failure")) | (_, Some("timed_out")) | (_, Some("cancelled")) => CheckStatus::Fail,
                    (Some("completed"), _) => CheckStatus::Fail,
                    _ => CheckStatus::Pending,
                };
                let required = required_names.contains(&name);
                let details_url = c["details_url"].as_str().map(String::from);
                Check { name, status, required, details_url, log_snippet: None }
            })
            .collect();

        // Append commit statuses (deduplicate by context — statuses API returns most-recent first)
        let mut seen_contexts: HashSet<String> = checks.iter().map(|c| c.name.clone()).collect();
        if let Some(statuses) = statuses_json.as_array() {
            for s in statuses {
                let name = s["context"].as_str().unwrap_or("").to_string();
                if name.is_empty() || seen_contexts.contains(&name) { continue; }
                seen_contexts.insert(name.clone());
                let status = match s["state"].as_str() {
                    Some("success") => CheckStatus::Pass,
                    Some("failure") | Some("error") => CheckStatus::Fail,
                    _ => CheckStatus::Pending,
                };
                let required = required_names.contains(&name);
                let details_url = s["target_url"].as_str().filter(|u| !u.is_empty()).map(String::from);
                checks.push(Check { name, status, required, details_url, log_snippet: None });
            }
        }

        // 4. Reviews → approval
        let reviews_json = self.get_paginated(&format!("/repos/{}/{}/pulls/{}/reviews", owner, repo, pr_number))?;
        let any_approved = reviews_json
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .any(|r| r["state"].as_str() == Some("APPROVED"));
        let rr = self.get(&format!("/repos/{}/{}/pulls/{}/requested_reviewers", owner, repo, pr_number))
            .unwrap_or_default();
        let rr_users = rr["users"].as_array().map(|a| a.len()).unwrap_or(0);
        let rr_teams = rr["teams"].as_array().map(|a| a.len()).unwrap_or(0);
        let approved = any_approved && rr_users == 0 && rr_teams == 0;

        // 4b. Review bodies → threads (CHANGES_REQUESTED/COMMENTED with non-empty body, non-bot, non-author)
        let pr_author = pr_json["user"]["login"].as_str().unwrap_or("").to_string();
        anyhow::ensure!(!pr_author.is_empty(), "could not determine PR author from GitHub response");

        // Fetch issue comments early so we can use them for review body thread state too
        let issue_comments_json = self.get_paginated(&format!("/repos/{}/{}/issues/{}/comments", owner, repo, pr_number))?;
        let issue_comments = issue_comments_json.as_array().cloned().unwrap_or_default();

        let mut review_body_threads: Vec<Thread> = Vec::new();
        for r in reviews_json.as_array().unwrap_or(&vec![]) {
            let state = r["state"].as_str().unwrap_or("");
            let body = r["body"].as_str().unwrap_or("");
            let login = r["user"]["login"].as_str().unwrap_or("");
            let user_type = r["user"]["type"].as_str().unwrap_or("");
            let is_bot = user_type == "Bot" || login.contains("[bot]");
            if (state == "CHANGES_REQUESTED" || state == "COMMENTED") && !body.is_empty() && !is_bot && login != pr_author {
                let submitted_at = r["submitted_at"].as_str().unwrap_or("");
                // Find the last comment after submitted_at from either the reviewer or the author.
                // If last speaker is the author → Addressed; if reviewer came back after → Open.
                let last_relevant = issue_comments.iter()
                    .rfind(|c| {
                        let commenter = c["user"]["login"].as_str().unwrap_or("");
                        let at = c["created_at"].as_str().unwrap_or("");
                        at > submitted_at && (commenter == pr_author || commenter == login)
                    });
                let thread_state = match last_relevant {
                    Some(c) if c["user"]["login"].as_str().unwrap_or("") == pr_author => ThreadState::Addressed,
                    _ => ThreadState::Open,
                };
                review_body_threads.push(Thread {
                    id: r["id"].as_u64().unwrap_or(0),
                    state: thread_state,
                    author: login.to_string(),
                    body: body.to_string(),
                    replies: vec![],
                    file: None,
                    line: None,
                });
            }
        }

        // 5. Review comments → threads grouped by root (in_reply_to_id == null)
        let comments_json = self.get_paginated(&format!("/repos/{}/{}/pulls/{}/comments", owner, repo, pr_number))?;
        let all_comments = comments_json.as_array().cloned().unwrap_or_default();

        // Build ordered list: (root_id, comments_in_thread) preserving API order
        let mut threads_map: Vec<(u64, Vec<&serde_json::Value>)> = Vec::new();
        // First pass: register root comments in order
        for c in &all_comments {
            if c.get("in_reply_to_id").and_then(|v| v.as_u64()).is_none() {
                let id = c["id"].as_u64().unwrap_or(0);
                threads_map.push((id, vec![c]));
            }
        }
        // Second pass: attach replies to their root
        for c in &all_comments {
            if let Some(root_id) = c.get("in_reply_to_id").and_then(|v| v.as_u64())
                && let Some(entry) = threads_map.iter_mut().find(|(k, _)| *k == root_id) {
                    entry.1.push(c);
                }
        }
        let threads: Vec<Thread> = threads_map.into_iter().map(|(_, comments)| {
            let root = comments[0];
            let last = comments.last().unwrap();
            let last_author = last["user"]["login"].as_str().unwrap_or("");
            let state = if last_author == pr_author {
                ThreadState::Addressed
            } else {
                ThreadState::Open
            };
            Thread {
                id: root["id"].as_u64().unwrap_or(0),
                state,
                author: root["user"]["login"].as_str().unwrap_or("").to_string(),
                body: root["body"].as_str().unwrap_or("").to_string(),
                replies: comments[1..].iter().map(|c| (
                    c["user"]["login"].as_str().unwrap_or("").to_string(),
                    c["body"].as_str().unwrap_or("").to_string(),
                )).collect(),
                file: root["path"].as_str().map(String::from),
                line: root["line"].as_u64().map(|l| l as u32),
            }
        }).collect();

        // 6. Issue-level comments (PR conversation, not inline review comments)
        // Surfaced as threads: Open if no author reply, Addressed if author replied. Bots and author-only threads excluded.
        let mut issue_threads: Vec<Thread> = Vec::new();
        // Each top-level issue comment is its own thread (no threading in issue comments API)
        // Group consecutive comments: first non-author comment starts a thread, replies follow until next non-author comment
        // Simpler model: each non-bot, non-author comment is a thread root; state = Addressed if author has replied after it
        let mut i = 0;
        while i < issue_comments.len() {
            let c = &issue_comments[i];
            let login = c["user"]["login"].as_str().unwrap_or("");
            let user_type = c["user"]["type"].as_str().unwrap_or("");
            let is_bot = user_type == "Bot" || login.contains("[bot]");
            let is_author = login == pr_author;
            if !is_bot && !is_author {
                // Check if author replied with an @mention of this reviewer
                let mention = format!("@{}", login);
                let author_replied = issue_comments[i+1..].iter().any(|r| {
                    r["user"]["login"].as_str().unwrap_or("") == pr_author
                        && r["body"].as_str().unwrap_or("").contains(&mention)
                });
                let state = if author_replied { ThreadState::Addressed } else { ThreadState::Open };
                issue_threads.push(Thread {
                    id: c["id"].as_u64().unwrap_or(0),
                    state,
                    author: login.to_string(),
                    body: c["body"].as_str().unwrap_or("").to_string(),
                    replies: vec![],
                    file: None,
                    line: None,
                });
            }
            i += 1;
        }
        let threads: Vec<Thread> = threads.into_iter().chain(review_body_threads).chain(issue_threads).collect();

        let has_merge_conflict = pr_json["mergeable"].as_bool() == Some(false);
        Ok(PrState { number: pr_number, title, branch, base: base_branch, head_sha, needs_parent_rebase: false, draft, approved, checks, threads, has_merge_conflict, codeowners_eligibility: Default::default() })
    }
}




pub use crate::upload::{github_upload_image, inject_demo_section};

/// Returns the repo's preferred merge method, using `cache` (keyed by "owner/repo") to avoid
/// repeated API calls. Queries GitHub and populates cache on first call per repo.
pub fn resolve_merge_method(
    client: &GithubClient,
    owner: &str,
    repo: &str,
    cache: &mut std::collections::HashMap<String, String>,
) -> Result<String> {
    let key = format!("{}/{}", owner, repo);
    if let Some(cached) = cache.get(&key) {
        return Ok(cached.clone());
    }
    let method = client.fetch_repo_merge_method(owner, repo)?;
    cache.insert(key, method.clone());
    Ok(method)
}

pub use crate::credentials::{extract_github_session_from_browser_with_chrome_db, resolve_github_token};
pub use crate::agent::agent_context_manifest_with_prs;

pub use crate::display::{format_open_threads, format_resolved_threads, fetch_open_threads};
pub use crate::model::{ResolvedThreadInfo, parse_resolved_review_threads_from_graphql};

pub use crate::merge::resolve_track_branch;

pub use crate::worktree::detect_repo;
#[cfg(test)]
pub use crate::worktree::parse_github_remote_pub;
