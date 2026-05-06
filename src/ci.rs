use anyhow::Result;

#[derive(Debug, Clone, PartialEq)]
pub enum CiProvider {
    GitHubActions { owner: String, repo: String, job_id: u64 },
    Buildkite { org: String, pipeline: String, build_num: u64 },
    Unknown(String),
}

pub fn parse_ci_provider(url: &str) -> CiProvider {
    let url_no_fragment = url.split('#').next().unwrap_or(url);

    if let Some(rest) = url_no_fragment.strip_prefix("https://github.com/") {
        let parts: Vec<&str> = rest.splitn(6, '/').collect();
        // owner/repo/actions/runs/{run_id}/jobs/{job_id}
        if parts.len() >= 6 && parts[2] == "actions" && parts[3] == "runs" && parts[5].starts_with("jobs") {
            // parts[5] = "jobs" is missing, let's re-check the split
        }
        // Try: owner repo actions runs {run_id} jobs/{job_id}  — need different split
        let parts: Vec<&str> = rest.split('/').collect();
        if parts.len() >= 7 && parts[2] == "actions" && parts[3] == "runs" && parts[5] == "jobs"
            && let Ok(job_id) = parts[6].parse::<u64>() {
            return CiProvider::GitHubActions {
                owner: parts[0].to_string(),
                repo: parts[1].to_string(),
                job_id,
            };
        }
    }

    if let Some(rest) = url_no_fragment.strip_prefix("https://buildkite.com/") {
        let parts: Vec<&str> = rest.split('/').collect();
        // org/pipeline/builds/{build_num}
        if parts.len() >= 4 && parts[2] == "builds"
            && let Ok(build_num) = parts[3].parse::<u64>() {
            return CiProvider::Buildkite {
                org: parts[0].to_string(),
                pipeline: parts[1].to_string(),
                build_num,
            };
        }
    }

    CiProvider::Unknown(url.to_string())
}

pub struct CiLogClient {
    github_token: String,
    github_base_url: String,
}

impl CiLogClient {
    pub fn new(github_token: String) -> Self {
        CiLogClient { github_token, github_base_url: "https://api.github.com".into() }
    }

    #[cfg(test)]
    pub fn with_base_url(github_token: String, github_base_url: String) -> Self {
        CiLogClient { github_token, github_base_url }
    }

    /// Fetch the complete raw log for a CI job, untruncated, for --full-log output.
    pub fn fetch_raw_log(&self, provider: &CiProvider) -> Result<String> {
        match provider {
            CiProvider::GitHubActions { owner, repo, job_id } => {
                let url = format!("{}/repos/{}/{}/actions/jobs/{}/logs", self.github_base_url, owner, repo, job_id);
                let resp = reqwest::blocking::Client::new()
                    .get(&url)
                    .header("Authorization", format!("Bearer {}", self.github_token))
                    .header("Accept", "application/vnd.github+json")
                    .header("User-Agent", "fp/0.1")
                    .header("X-GitHub-Api-Version", "2022-11-28")
                    .send()?;
                if resp.status().is_redirection()
                    && let Some(location) = resp.headers().get("Location") {
                    let log_url = location.to_str()?;
                    return Ok(reqwest::blocking::Client::new().get(log_url).send()?.error_for_status()?.text()?);
                }
                Ok(resp.error_for_status()?.text()?)
            }
            CiProvider::Buildkite { org, pipeline, build_num } => {
                let token = std::env::var("BUILDKITE_TOKEN").ok()
                    .ok_or_else(|| anyhow::anyhow!("BUILDKITE_TOKEN not set"))?;
                let url = format!("https://api.buildkite.com/v2/organizations/{}/pipelines/{}/builds/{}", org, pipeline, build_num);
                let resp = reqwest::blocking::Client::new()
                    .get(&url)
                    .header("Authorization", format!("Bearer {}", token))
                    .header("User-Agent", "fp/0.1")
                    .send()?.error_for_status()?.json::<serde_json::Value>()?;
                let mut output = String::new();
                if let Some(jobs) = resp["jobs"].as_array() {
                    for job in jobs {
                        if job["state"].as_str() == Some("failed") {
                            if let Some(log_url) = job["raw_log_url"].as_str() {
                                let text = reqwest::blocking::Client::new()
                                    .get(log_url).header("Authorization", format!("Bearer {}", token))
                                    .send()?.text()?;
                                output.push_str(&format!("=== {} ===\n{}\n", job["name"].as_str().unwrap_or("job"), text));
                            }
                        }
                    }
                }
                Ok(output)
            }
            CiProvider::Unknown(url) => Ok(format!("Log URL: {}\n(full log not available for unknown provider)", url)),
        }
    }

    pub fn fetch_logs(&self, provider: &CiProvider) -> Result<String> {
        match provider {
            CiProvider::GitHubActions { owner, repo, job_id } => {
                self.fetch_github_actions_logs(owner, repo, *job_id)
            }
            CiProvider::Buildkite { org, pipeline, build_num } => {
                self.fetch_buildkite_logs(org, pipeline, *build_num)
            }
            CiProvider::Unknown(url) => {
                Ok(format!("Log URL: {}\n(Automatic log fetching not supported for this provider)", url))
            }
        }
    }

    fn fetch_github_actions_logs(&self, owner: &str, repo: &str, job_id: u64) -> Result<String> {
        let url = format!("{}/repos/{}/{}/actions/jobs/{}/logs", self.github_base_url, owner, repo, job_id);
        let resp = reqwest::blocking::Client::new()
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.github_token))
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "fp/0.1")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()?;

        // GitHub returns 302 redirect to actual log content
        if resp.status().is_redirection()
            && let Some(location) = resp.headers().get("Location") {
            let log_url = location.to_str()?;
            let log_resp = reqwest::blocking::Client::new()
                .get(log_url)
                .send()?
                .error_for_status()?;
            let text = log_resp.text()?;
            return Ok(tail_lines(&text, 100));
        }

        let text = resp.error_for_status()?.text()?;
        Ok(tail_lines(&text, 100))
    }

    fn fetch_buildkite_logs(&self, org: &str, pipeline: &str, build_num: u64) -> Result<String> {
        let token = std::env::var("BUILDKITE_TOKEN").ok();
        if let Some(tok) = token {
            let url = format!(
                "https://api.buildkite.com/v2/organizations/{}/pipelines/{}/builds/{}",
                org, pipeline, build_num
            );
            let resp = reqwest::blocking::Client::new()
                .get(&url)
                .header("Authorization", format!("Bearer {}", tok))
                .header("User-Agent", "fp/0.1")
                .send()?
                .error_for_status()?
                .json::<serde_json::Value>()?;

            // Collect structured extraction from failed jobs
            let build_url = format!("https://buildkite.com/{}/{}/builds/{}", org, pipeline, build_num);
            let mut results: Vec<BuildkiteLogResult> = Vec::new();
            if let Some(jobs) = resp["jobs"].as_array() {
                for job in jobs {
                    if job["state"].as_str() == Some("failed") {
                        let name = job["name"].as_str().unwrap_or("unknown").to_string();
                        let job_url = job["web_url"].as_str().unwrap_or(&build_url).to_string();
                        let raw_log = if let Some(log_url) = job["raw_log_url"].as_str() {
                            reqwest::blocking::Client::new()
                                .get(log_url)
                                .header("Authorization", format!("Bearer {}", tok))
                                .send()
                                .ok()
                                .and_then(|r| r.text().ok())
                                .unwrap_or_default()
                        } else {
                            String::new()
                        };
                        results.push(extract_buildkite_log(&raw_log, &name, &job_url));
                    }
                }
            }
            if results.is_empty() {
                Ok(format!("Build #{} — no failed jobs with logs found\nBuild URL: {}", build_num, build_url))
            } else {
                Ok(serde_json::to_string_pretty(&results).unwrap_or_else(|_| format!("{} failed jobs", results.len())))
            }
        } else {
            Ok(format!(
                "Log URL: https://buildkite.com/{}/{}/builds/{}\nSet BUILDKITE_TOKEN to fetch logs automatically.",
                org, pipeline, build_num
            ))
        }
    }
}

fn tail_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join("\n")
}

#[derive(Debug, serde::Serialize)]
pub struct BuildkiteLogResult {
    pub step: String,
    pub error_lines: Vec<String>,
    pub context_lines: Vec<String>,
    pub log_url: String,
    pub full_log_available: bool,
}

const ERROR_PATTERNS: &[&str] = &["Error:", "error:", "FAILED", "FAIL", "panic:", "exception", "Exception"];

/// Extract a structured summary from a raw Buildkite log for a named step.
pub fn extract_buildkite_log(raw: &str, step: &str, log_url: &str) -> BuildkiteLogResult {
    let lines: Vec<&str> = raw.lines().collect();
    let error_lines: Vec<String> = lines.iter()
        .filter(|l| ERROR_PATTERNS.iter().any(|p| l.contains(p)))
        .map(|l| l.to_string())
        .collect();
    let context_lines: Vec<String> = tail_lines(raw, 50)
        .lines()
        .map(String::from)
        .collect();
    BuildkiteLogResult {
        step: step.to_string(),
        error_lines,
        context_lines,
        log_url: log_url.to_string(),
        full_log_available: true,
    }
}
