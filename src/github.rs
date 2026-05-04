use anyhow::Result;
use std::collections::HashSet;
use crate::model::{Check, CheckStatus, PrState, Thread, ThreadState};

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

    pub fn create_pr(&self, owner: &str, repo: &str, title: &str, head: &str, base: &str, draft: bool) -> Result<PrState> {
        let url = format!("{}/repos/{}/{}/pulls", self.base_url, owner, repo);
        let payload = serde_json::json!({ "title": title, "head": head, "base": base, "draft": draft });
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
            draft: resp["draft"].as_bool().unwrap_or(false),
            approved: false,
            checks: vec![],
            threads: vec![],
        })
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

        // 2. Check runs
        let encoded_branch = branch.replace('/', "%2F");
        let checks_json = self.get(&format!(
            "/repos/{}/{}/commits/{}/check-runs", owner, repo, encoded_branch
        ))?;

        // 3. Branch protection — required check names (404 = no protection configured)
        let required_names: HashSet<String> = self
            .get(&format!("/repos/{}/{}/branches/{}/protection", owner, repo, encoded_branch))
            .ok()
            .and_then(|j| {
                j["required_status_checks"]["contexts"]
                    .as_array()
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            })
            .unwrap_or_default();

        let checks: Vec<Check> = checks_json["check_runs"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .map(|c| {
                let name = c["name"].as_str().unwrap_or("").to_string();
                let status = match (c["status"].as_str(), c["conclusion"].as_str()) {
                    (_, Some("success")) => CheckStatus::Pass,
                    (_, Some("failure")) | (_, Some("timed_out")) | (_, Some("cancelled")) => CheckStatus::Fail,
                    (Some("completed"), _) => CheckStatus::Fail,
                    _ => CheckStatus::Pending,
                };
                let required = required_names.contains(&name);
                let details_url = c["details_url"].as_str().map(String::from);
                Check { name, status, required, details_url }
            })
            .collect();

        // 4. Reviews → approval
        let reviews_json = self.get(&format!("/repos/{}/{}/pulls/{}/reviews", owner, repo, pr_number))?;
        let approved = reviews_json
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .any(|r| r["state"].as_str() == Some("APPROVED"));

        // 5. Review comments → threads grouped by root (in_reply_to_id == null)
        let pr_author = pr_json["user"]["login"].as_str().unwrap_or("").to_string();
        let comments_json = self.get(&format!("/repos/{}/{}/pulls/{}/comments", owner, repo, pr_number))?;
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
            if let Some(root_id) = c.get("in_reply_to_id").and_then(|v| v.as_u64()) {
                if let Some(entry) = threads_map.iter_mut().find(|(k, _)| *k == root_id) {
                    entry.1.push(c);
                }
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
                body: root["body"].as_str().unwrap_or("").to_string(),
                file: root["path"].as_str().map(String::from),
                line: root["line"].as_u64().map(|l| l as u32),
            }
        }).collect();

        Ok(PrState { number: pr_number, title, branch, draft, approved, checks, threads })
    }
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
