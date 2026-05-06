#[cfg(test)]
mod tests {
    use crate::ci::{parse_ci_provider, CiProvider};

    // C1: GitHub Actions URL parsed correctly
    #[test]
    fn parse_github_actions_url() {
        let url = "https://github.com/owner/repo/actions/runs/123456/jobs/789012";
        let provider = parse_ci_provider(url);
        assert_eq!(
            provider,
            CiProvider::GitHubActions {
                owner: "owner".into(),
                repo: "repo".into(),
                job_id: 789012,
            }
        );
    }

    // C2: Buildkite URL parsed correctly
    #[test]
    fn parse_buildkite_url() {
        let url = "https://buildkite.com/myorg/my-pipeline/builds/42";
        let provider = parse_ci_provider(url);
        assert_eq!(
            provider,
            CiProvider::Buildkite {
                org: "myorg".into(),
                pipeline: "my-pipeline".into(),
                build_num: 42,
            }
        );
    }

    // C2: Buildkite URL with fragment (#step) parsed correctly
    #[test]
    fn parse_buildkite_url_with_fragment() {
        let url = "https://buildkite.com/myorg/my-pipeline/builds/42#step-test";
        let provider = parse_ci_provider(url);
        assert_eq!(
            provider,
            CiProvider::Buildkite {
                org: "myorg".into(),
                pipeline: "my-pipeline".into(),
                build_num: 42,
            }
        );
    }

    // C3: Unknown provider URL returned as-is
    #[test]
    fn parse_unknown_url() {
        let url = "https://circleci.com/gh/owner/repo/123";
        let provider = parse_ci_provider(url);
        assert_eq!(provider, CiProvider::Unknown(url.into()));
    }

    // C4: GitHub Actions fetch_logs calls correct API endpoint
    #[test]
    fn github_actions_fetch_logs_calls_correct_endpoint() {
        let mut server = mockito::Server::new();
        // GitHub Actions /logs endpoint returns a redirect (302) to the actual log URL.
        // The client should follow the redirect and return the log text.
        let log_url = format!("{}/log-content", server.url());
        server.mock("GET", "/repos/owner/repo/actions/jobs/789/logs")
            .with_status(302)
            .with_header("Location", &log_url)
            .create();
        server.mock("GET", "/log-content")
            .with_status(200)
            .with_header("content-type", "text/plain")
            .with_body("line1\nline2\nERROR: build failed\n")
            .create();

        let client = crate::ci::CiLogClient::with_base_url("test-token".into(), server.url());
        let logs = client.fetch_logs(&CiProvider::GitHubActions {
            owner: "owner".into(),
            repo: "repo".into(),
            job_id: 789,
        }).unwrap();
        assert!(logs.contains("ERROR: build failed"));
    }

    // C5: Buildkite fetch_logs returns URL string when no BUILDKITE_TOKEN
    #[test]
    fn buildkite_no_token_returns_url() {
        // Ensure env var is not set for this test
        // SAFETY: test-only, single-threaded test context
        unsafe { std::env::remove_var("BUILDKITE_TOKEN"); }
        let client = crate::ci::CiLogClient::with_base_url("github-token".into(), "http://unused".into());
        let result = client.fetch_logs(&CiProvider::Buildkite {
            org: "org".into(),
            pipeline: "pipe".into(),
            build_num: 1,
        });
        // Should return Ok with a message about the URL, not an error
        let text = result.unwrap();
        assert!(text.contains("buildkite.com") || text.contains("BUILDKITE_TOKEN"));
    }

    // C3: Unknown provider fetch_logs returns URL only
    #[test]
    fn unknown_provider_returns_url() {
        let client = crate::ci::CiLogClient::with_base_url("token".into(), "http://unused".into());
        let url = "https://example.com/build/123";
        let result = client.fetch_logs(&CiProvider::Unknown(url.into())).unwrap();
        assert!(result.contains(url));
    }

    // D6-a: extract_buildkite_log returns structured BuildkiteLogResult with step, error_lines, context_lines, log_url
    #[test]
    fn extract_buildkite_log_returns_structured_result() {
        use crate::ci::extract_buildkite_log;
        let raw_log = "Starting step rspec\nline1\nline2\nError: test failed\nline4\npanic: something bad";
        let result = extract_buildkite_log(raw_log, "rspec", "https://buildkite.com/org/pipe/builds/1");
        assert_eq!(result.step, "rspec");
        assert!(result.error_lines.iter().any(|l| l.contains("Error: test failed")),
            "error_lines should contain Error: line, got: {:?}", result.error_lines);
        assert!(result.error_lines.iter().any(|l| l.contains("panic:")),
            "error_lines should contain panic: line, got: {:?}", result.error_lines);
        assert_eq!(result.log_url, "https://buildkite.com/org/pipe/builds/1");
    }

    // D6-b: extract_buildkite_log log_url is always present even when no errors found
    #[test]
    fn extract_buildkite_log_log_url_always_present() {
        use crate::ci::extract_buildkite_log;
        let result = extract_buildkite_log("all good\nno errors here", "build", "https://buildkite.com/x/y/builds/5");
        assert_eq!(result.log_url, "https://buildkite.com/x/y/builds/5");
        assert!(result.error_lines.is_empty());
    }

    // D6-c: truncated logs include a --full-log hint; short logs do not
    #[test]
    fn truncation_hint_present_iff_log_was_truncated() {
        let mut server = mockito::Server::new();

        // 200-line log → truncated → hint must appear
        let long_log_url = format!("{}/long-log", server.url());
        server.mock("GET", "/repos/owner/repo/actions/jobs/10/logs")
            .with_status(302).with_header("Location", &long_log_url).create();
        let long_log: String = (1..=200).map(|i| format!("line {}", i)).collect::<Vec<_>>().join("\n");
        server.mock("GET", "/long-log")
            .with_status(200).with_header("content-type", "text/plain")
            .with_body(&long_log).create();

        // 50-line log → not truncated → no hint
        let short_log_url = format!("{}/short-log", server.url());
        server.mock("GET", "/repos/owner/repo/actions/jobs/11/logs")
            .with_status(302).with_header("Location", &short_log_url).create();
        let short_log: String = (1..=50).map(|i| format!("line {}", i)).collect::<Vec<_>>().join("\n");
        server.mock("GET", "/short-log")
            .with_status(200).with_header("content-type", "text/plain")
            .with_body(&short_log).create();

        let client = crate::ci::CiLogClient::with_base_url("tok".into(), server.url());

        let long_result = client.fetch_logs(&CiProvider::GitHubActions {
            owner: "owner".into(), repo: "repo".into(), job_id: 10,
        }).unwrap();
        assert!(long_result.contains("--full-log"),
            "truncated log (200 lines) should contain --full-log hint, got: {}", long_result);

        let short_result = client.fetch_logs(&CiProvider::GitHubActions {
            owner: "owner".into(), repo: "repo".into(), job_id: 11,
        }).unwrap();
        assert!(!short_result.contains("--full-log"),
            "short log (50 lines) should NOT contain --full-log hint, got: {}", short_result);
    }

    // D6-b (fetch_raw_log): GitHub Actions fetch_raw_log returns full untruncated log (not tail)
    #[test]
    fn fetch_raw_log_github_actions_returns_full_text() {
        let mut server = mockito::Server::new();
        let log_url = format!("{}/full-log-content", server.url());
        // GitHub returns 302 redirect to actual log
        server.mock("GET", "/repos/owner/repo/actions/jobs/42/logs")
            .with_status(302)
            .with_header("Location", &log_url)
            .create();
        // Full log has 200 lines — fetch_raw_log must return all, not just tail-100
        let full_log: String = (1..=200).map(|i| format!("line {}", i)).collect::<Vec<_>>().join("\n");
        server.mock("GET", "/full-log-content")
            .with_status(200)
            .with_header("content-type", "text/plain")
            .with_body(&full_log)
            .create();

        let client = crate::ci::CiLogClient::with_base_url("tok".into(), server.url());
        let result = client.fetch_raw_log(&CiProvider::GitHubActions {
            owner: "owner".into(), repo: "repo".into(), job_id: 42,
        }).unwrap();
        assert!(result.contains("line 1\n"), "full log should contain line 1 (start), got {} chars", result.len());
        assert!(result.contains("line 200"), "full log should contain line 200 (end), got {} chars", result.len());
    }
}
