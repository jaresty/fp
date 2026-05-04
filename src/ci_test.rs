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
}
