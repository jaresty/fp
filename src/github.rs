use anyhow::Result;
use std::collections::HashSet;
use crate::model::{Check, CheckStatus, PrState, Thread, ThreadState};
use crate::store::TrackedPr;

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

    fn get(&self, path: &str) -> Result<serde_json::Value> {
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
            draft: resp["draft"].as_bool().unwrap_or(false),
            approved: false,
            checks: vec![],
            threads: vec![],
            has_merge_conflict: false, codeowners_eligibility: Default::default(),
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
                // Check if author replied in any subsequent comment
                let author_replied = issue_comments[i+1..].iter().any(|r| {
                    r["user"]["login"].as_str().unwrap_or("") == pr_author
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
        Ok(PrState { number: pr_number, title, branch, base: base_branch, draft, approved, checks, threads, has_merge_conflict, codeowners_eligibility: Default::default() })
    }
}


pub fn derive_chrome_aes_key(password: &[u8]) -> [u8; 16] {
    use hmac::Hmac;
    use sha1::Sha1;
    let mut key = [0u8; 16];
    pbkdf2::pbkdf2::<Hmac<Sha1>>(password, b"saltysalt", 1003, &mut key);
    key
}

pub fn decrypt_chrome_cookie(encrypted: &[u8], key: &[u8; 16]) -> Result<String> {
    use aes::cipher::{BlockDecrypt, KeyInit, generic_array::GenericArray};
    use aes::Aes128;
    if encrypted.len() < 3 + 16 { anyhow::bail!("encrypted value too short"); }
    // Strip 3-byte "v10" prefix; IV is always 16 spaces (hardcoded by Chrome on macOS)
    let iv: [u8; 16] = [b' '; 16];
    let ciphertext = &encrypted[3..];
    if !ciphertext.len().is_multiple_of(16) { anyhow::bail!("ciphertext not block-aligned"); }
    let cipher = Aes128::new(GenericArray::from_slice(key));
    let mut plaintext = ciphertext.to_vec();
    // CBC decrypt: each block XOR'd with previous ciphertext block (or IV for first)
    for i in (0..plaintext.len()).step_by(16) {
        let prev: [u8; 16] = if i == 0 { iv } else { ciphertext[i-16..i].try_into()? };
        let block = GenericArray::from_mut_slice(&mut plaintext[i..i+16]);
        cipher.decrypt_block(block);
        for j in 0..16 { plaintext[i+j] ^= prev[j]; }
    }
    // PKCS7 unpad
    let pad = *plaintext.last().ok_or_else(|| anyhow::anyhow!("empty plaintext"))? as usize;
    if pad == 0 || pad > 16 || pad > plaintext.len() { anyhow::bail!("invalid PKCS7 padding: pad={}", pad); }
    plaintext.truncate(plaintext.len() - pad);
    // Chrome DB v24+ prepends a 32-byte prefix before the actual cookie value
    if plaintext.len() > 32 {
        plaintext.drain(..32);
    }
    Ok(String::from_utf8(plaintext)?)
}

pub fn read_chrome_user_session_encrypted(db_path: &std::path::Path) -> Result<Vec<u8>> {
    let conn = rusqlite::Connection::open(db_path)?;
    let blob: Vec<u8> = conn.query_row(
        "SELECT encrypted_value FROM cookies WHERE host_key LIKE '%github.com' AND name = 'user_session' LIMIT 1",
        [],
        |row| row.get(0),
    ).map_err(|e| anyhow::anyhow!("user_session cookie not found in Chrome cookies DB: {}", e))?;
    Ok(blob)
}

#[cfg(target_os = "macos")]
fn get_chrome_safe_storage_password() -> Result<String> {
    use security_framework::passwords::get_generic_password;
    let bytes = get_generic_password("Chrome Safe Storage", "Chrome")
        .map_err(|e| anyhow::anyhow!("failed to read Chrome Safe Storage from Keychain: {}", e))?;
    Ok(String::from_utf8(bytes)?)
}

#[cfg(target_os = "macos")]
pub fn extract_github_session_from_browser_with_chrome_db(chrome_db: &std::path::Path) -> Result<String> {
    if chrome_db.exists() {
        let password = get_chrome_safe_storage_password()?;
        let key = derive_chrome_aes_key(password.as_bytes());
        let encrypted = read_chrome_user_session_encrypted(chrome_db)?;
        return decrypt_chrome_cookie(&encrypted, &key);
    }
    anyhow::bail!("no GitHub session found — set GITHUB_USER_SESSION env var or log into GitHub in Chrome")
}

#[cfg(not(target_os = "macos"))]
pub fn extract_github_session_from_browser_with_chrome_db(_chrome_db: &std::path::Path) -> Result<String> {
    anyhow::bail!("no GitHub session found — set GITHUB_USER_SESSION env var or log into GitHub in Chrome")
}


pub fn parse_upload_token(html: &str) -> Result<String> {
    let marker = r#""uploadToken":""#;
    if let Some(start) = html.find(marker) {
        let rest = &html[start + marker.len()..];
        if let Some(end) = rest.find('"') {
            return Ok(rest[..end].to_string());
        }
    }
    anyhow::bail!("uploadToken not found in page HTML")
}

#[allow(dead_code)]
pub struct UploadPolicy {
    pub upload_url: String,
    pub asset_id: u64,
    pub asset_href: String,
    pub asset_upload_authenticity_token: String,
    pub form_fields: std::collections::HashMap<String, String>,
}

pub fn parse_upload_policy_response(json: &str) -> Result<UploadPolicy> {
    let v: serde_json::Value = serde_json::from_str(json)?;
    let upload_url = v["upload_url"].as_str().ok_or_else(|| anyhow::anyhow!("missing upload_url"))?.to_string();
    let asset_id = v["asset"]["id"].as_u64().ok_or_else(|| anyhow::anyhow!("missing asset.id"))?;
    let asset_href = v["asset"]["href"].as_str().ok_or_else(|| anyhow::anyhow!("missing asset.href"))?.to_string();
    let asset_upload_authenticity_token = v["asset_upload_authenticity_token"].as_str().ok_or_else(|| anyhow::anyhow!("missing asset_upload_authenticity_token"))?.to_string();
    let form_fields = v["form"].as_object()
        .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string())).collect())
        .unwrap_or_default();
    Ok(UploadPolicy { upload_url, asset_id, asset_href, asset_upload_authenticity_token, form_fields })
}

/// Uploads a local image file to GitHub using the undocumented 3-step asset upload flow.
/// Requires a `user_session` browser cookie (set GITHUB_USER_SESSION env var).
/// Returns the final asset URL (https://github.com/user-attachments/assets/...).
pub fn github_upload_image(
    file_path: &std::path::Path,
    owner: &str,
    repo: &str,
    api_client: &GithubClient,
    session_cookie: &str,
) -> Result<String> {
    let file_bytes = std::fs::read(file_path)?;
    let filename = file_path.file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow::anyhow!("invalid filename"))?;
    let content_type = mime_type_from_filename(filename);

    // Get repository_id from API
    let repo_info = api_client.get(&format!("/repos/{}/{}", owner, repo))?;
    let repo_id = repo_info["id"].as_u64().ok_or_else(|| anyhow::anyhow!("missing repo id"))?;

    let cookie_header = format!(
        "user_session={}; __Host-user_session_same_site={}",
        session_cookie, session_cookie
    );

    let http = reqwest::blocking::Client::builder()
        .user_agent("fp/0.1")
        .build()?;

    // Step 0: get uploadToken from repo page HTML
    let page_html = http.get(format!("https://github.com/{}/{}", owner, repo))
        .header("Cookie", &cookie_header)
        .send()?
        .text()?;
    let upload_token = parse_upload_token(&page_html)?;

    // Step 1: POST upload policy
    let policy_resp = http.post("https://github.com/upload/policies/assets")
        .header("Cookie", &cookie_header)
        .header("Accept", "application/json")
        .header("Origin", "https://github.com")
        .header("Referer", format!("https://github.com/{}/{}", owner, repo))
        .header("X-Requested-With", "XMLHttpRequest")
        .multipart(
            reqwest::blocking::multipart::Form::new()
                .text("name", filename.to_string())
                .text("size", file_bytes.len().to_string())
                .text("content_type", content_type.to_string())
                .text("authenticity_token", upload_token)
                .text("repository_id", repo_id.to_string()),
        )
        .send()?
        .text()?;
    let policy = parse_upload_policy_response(&policy_resp)?;

    // Step 2: POST to S3 with presigned form fields + file binary last
    let mut form = reqwest::blocking::multipart::Form::new();
    // S3 requires form fields in order; add in the order they appear
    for (k, v) in &policy.form_fields {
        form = form.text(k.clone(), v.clone());
    }
    form = form.part("file", reqwest::blocking::multipart::Part::bytes(file_bytes)
        .file_name(filename.to_string())
        .mime_str(content_type)?);
    let s3_status = http.post(&policy.upload_url)
        .multipart(form)
        .send()?
        .status();
    if !s3_status.is_success() && s3_status.as_u16() != 204 {
        anyhow::bail!("S3 upload failed with status {}", s3_status);
    }

    // Step 3: finalize asset
    let finalize_resp: serde_json::Value = http
        .put(format!("https://github.com/upload/assets/{}", policy.asset_id))
        .header("Cookie", &cookie_header)
        .header("Accept", "application/json")
        .multipart(
            reqwest::blocking::multipart::Form::new()
                .text("authenticity_token", policy.asset_upload_authenticity_token),
        )
        .send()?
        .json()?;
    let href = finalize_resp["href"].as_str()
        .ok_or_else(|| anyhow::anyhow!("missing href in finalize response"))?
        .to_string();
    Ok(href)
}

fn mime_type_from_filename(filename: &str) -> &'static str {
    match filename.rsplit('.').next().unwrap_or("").to_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

#[allow(dead_code)]
/// Parses the markdown output of `gh image` (e.g. `![name](url)\n`) and returns the URL.
pub fn parse_gh_image_output(output: &str) -> Result<String> {
    for line in output.lines() {
        if let Some(start) = line.find("](")
            && let Some(end) = line[start + 2..].find(')')
        {
            return Ok(line[start + 2..start + 2 + end].to_string());
        }
    }
    anyhow::bail!("could not parse URL from gh image output: {:?}", output)
}

/// Injects or replaces a `## Demo` section in a PR body with numbered image markdown entries.
pub fn inject_demo_section(body: &str, urls: &[String]) -> String {
    let demo_section = {
        let images: String = urls.iter().enumerate()
            .map(|(i, url)| format!("![Demo {}]({})", i + 1, url))
            .collect::<Vec<_>>()
            .join("\n");
        format!("## Demo\n\n{}", images)
    };
    // Replace existing ## Demo section (and everything after it until next ## or end)
    if let Some(pos) = body.find("\n## Demo") {
        format!("{}\n\n{}", body[..pos].trim_end(), demo_section)
    } else if body.contains("## Demo") && body.starts_with("## Demo") {
        demo_section
    } else {
        format!("{}\n\n{}", body.trim_end(), demo_section)
    }
}

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

/// Resolve GitHub token: GITHUB_TOKEN env → gh CLI → hard error with enumerated remediation.
/// `env_token` and `gh_token` are injectable for tests; pass `None` to use live sources.
pub fn resolve_github_token_with(
    env_token: Option<String>,
    gh_token: Option<String>,
) -> Result<String> {
    if let Some(t) = env_token.filter(|s| !s.is_empty()) {
        return Ok(t);
    }
    if let Some(t) = gh_token.filter(|s| !s.is_empty()) {
        return Ok(t);
    }
    anyhow::bail!(
        "fp: no GitHub credentials found.\n  Option 1: export GITHUB_TOKEN=<token>\n  Option 2: gh auth login"
    )
}

/// Build the machine-readable capability manifest for `fp agent-context`.
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

pub fn agent_context_manifest_with_prs(prs: &[TrackedPr]) -> serde_json::Value {
    let mut manifest = agent_context_manifest();
    manifest["tracked_prs"] = serde_json::json!(prs
        .iter()
        .map(|p| serde_json::json!({"number": p.number, "title": p.title, "branch": p.branch}))
        .collect::<Vec<_>>());
    manifest
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

pub fn format_resolved_threads(pr: u64, threads: &[ResolvedThreadInfo], json: bool) -> String {
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

/// Return only Open and Stale threads — threads requiring author action for `fp threads`.
pub fn fetch_open_threads(threads: &[crate::model::Thread]) -> Vec<&crate::model::Thread> {
    threads.iter()
        .filter(|t| matches!(t.state, crate::model::ThreadState::Open | crate::model::ThreadState::Stale))
        .collect()
}


/// Resolved thread data from GitHub GraphQL API.
#[derive(Debug, Clone)]
pub struct ResolvedThreadInfo {
    pub body: String,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub resolved_by: Option<String>,
    pub created_at: Option<String>,
    pub first_commit_after_opened: Option<String>,
}

/// Parse resolved review threads from GitHub GraphQL response.
/// Returns threads where isResolved=true, including resolver, created_at, and first commit after.
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

/// Resolve the branch for a tracked PR. Prefers explicit, then fetched; errors with corrective
/// message when both are absent.
pub fn resolve_track_branch(
    explicit: Option<String>,
    fetched: Option<String>,
    pr_number: u64,
) -> Result<String> {
    if let Some(b) = explicit.filter(|s| !s.is_empty()) { return Ok(b); }
    if let Some(b) = fetched.filter(|s| !s.is_empty()) { return Ok(b); }
    anyhow::bail!(
        "fp: could not determine branch for PR #{}.\nRun: fp track {} --branch <branch-name>",
        pr_number, pr_number
    )
}

/// Production token resolution: reads GITHUB_TOKEN env, falls back to `gh auth token`.
pub fn resolve_github_token() -> Result<String> {
    let env_token = std::env::var("GITHUB_TOKEN").ok();
    let gh_token = std::process::Command::new("gh")
        .args(["auth", "token"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string());
    resolve_github_token_with(env_token, gh_token)
}

/// Detect owner/repo from `git remote get-url origin`
pub fn detect_repo() -> Option<(String, String)> {
    let output = std::process::Command::new("git")
        .args(["remote", "get-url", "origin"])
        .output()
        .ok()?;
    if !output.status.success() { return None; }
    let url = String::from_utf8(output.stdout).ok()?;
    parse_github_remote(url.trim())
}

#[cfg(test)]
pub fn parse_github_remote_pub(url: &str) -> Option<(String, String)> {
    parse_github_remote(url)
}

fn parse_github_remote(url: &str) -> Option<(String, String)> {
    // https://github.com/owner/repo.git  or  git@github.com:owner/repo.git
    let url = url.trim_end_matches(".git");
    if let Some(rest) = url.strip_prefix("https://github.com/") {
        let parts: Vec<&str> = rest.splitn(2, '/').collect();
        if parts.len() == 2 {
            return Some((parts[0].to_string(), parts[1].to_string()));
        }
    }
    if let Some(rest) = url.strip_prefix("git@github.com:") {
        let parts: Vec<&str> = rest.splitn(2, '/').collect();
        if parts.len() == 2 {
            return Some((parts[0].to_string(), parts[1].to_string()));
        }
    }
    None
}
