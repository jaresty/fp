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

    // C4: GitHub Actions fetch_raw_log follows redirect and returns full log text
    #[test]
    fn github_actions_fetch_raw_log_follows_redirect() {
        let mut server = mockito::Server::new();
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
        let raw = client.fetch_raw_log(&CiProvider::GitHubActions {
            owner: "owner".into(), repo: "repo".into(), job_id: 789,
        }).unwrap();
        assert!(raw.contains("ERROR: build failed"));
    }

    // C5: Buildkite fetch_raw_log returns Err when no BUILDKITE_TOKEN
    #[test]
    fn buildkite_no_token_returns_err() {
        // SAFETY: test-only, single-threaded test context
        unsafe { std::env::remove_var("BUILDKITE_TOKEN"); }
        let client = crate::ci::CiLogClient::with_base_url("github-token".into(), "http://unused".into());
        let result = client.fetch_raw_log(&CiProvider::Buildkite {
            org: "org".into(), pipeline: "pipe".into(), build_num: 1,
        });
        assert!(result.is_err(), "fetch_raw_log for Buildkite without token should return Err");
    }

    // C3: Unknown provider fetch_raw_log returns URL in message
    #[test]
    fn unknown_provider_returns_url() {
        let client = crate::ci::CiLogClient::with_base_url("token".into(), "http://unused".into());
        let url = "https://example.com/build/123";
        let result = client.fetch_raw_log(&CiProvider::Unknown(url.into())).unwrap();
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

    // D6-c: fetch_raw_log returns full log content without truncation
    #[test]
    fn fetch_raw_log_returns_full_content() {
        let mut server = mockito::Server::new();
        let log_url = format!("{}/full-log", server.url());
        server.mock("GET", "/repos/owner/repo/actions/jobs/42/logs")
            .with_status(302).with_header("Location", &log_url).create();
        let full_log: String = (1..=200).map(|i| format!("line {}", i)).collect::<Vec<_>>().join("\n");
        server.mock("GET", "/full-log")
            .with_status(200).with_header("content-type", "text/plain")
            .with_body(&full_log).create();

        let client = crate::ci::CiLogClient::with_base_url("tok".into(), server.url());
        let result = client.fetch_raw_log(&CiProvider::GitHubActions {
            owner: "owner".into(), repo: "repo".into(), job_id: 42,
        }).unwrap();
        assert!(result.contains("line 1\n"), "full log should contain line 1 (start), got {} chars", result.len());
        assert!(result.contains("line 200"), "full log should contain line 200 (end), got {} chars", result.len());
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

    // ADR-006: ESLint format lines are recognized in error_lines alongside existing patterns
    #[test]
    fn extract_buildkite_log_recognizes_eslint_errors() {
        use crate::ci::extract_buildkite_log;
        let raw_log = concat!(
            "Starting lint step\n",
            "src/index.js:10:5  error  'foo' is not defined  no-undef\n",
            "src/utils.ts:42:1  error  Missing semicolon  semi\n",
            "Error: lint failed\n",
            "Done.",
        );
        let result = extract_buildkite_log(raw_log, "lint", "https://buildkite.com/x/y/builds/7");
        assert!(
            result.error_lines.iter().any(|l| l.contains("src/index.js")),
            "error_lines should contain ESLint line for src/index.js, got: {:?}", result.error_lines
        );
        assert!(
            result.error_lines.iter().any(|l| l.contains("src/utils.ts")),
            "error_lines should contain ESLint line for src/utils.ts, got: {:?}", result.error_lines
        );
        assert!(
            result.error_lines.iter().any(|l| l.contains("Error: lint failed")),
            "error_lines should still contain existing Error: pattern, got: {:?}", result.error_lines
        );
    }

    // ADR-006: non-ESLint, non-error lines are excluded from error_lines
    #[test]
    fn extract_buildkite_log_excludes_non_error_lines() {
        use crate::ci::extract_buildkite_log;
        let raw_log = "Starting step\nsrc/index.js:10:5  warning  foo  some-rule\nnormal output\nDone.";
        let result = extract_buildkite_log(raw_log, "lint", "https://buildkite.com/x");
        assert!(
            result.error_lines.is_empty(),
            "warning-only ESLint lines and normal lines should not appear in error_lines, got: {:?}", result.error_lines
        );
    }

    // ADR-002 #6: format_context_output includes --full-log hint when full_log_available
    #[test]
    fn format_context_output_includes_full_log_hint_when_available() {
        use crate::ci::{format_context_output, BuildkiteLogResult};
        let result = BuildkiteLogResult {
            step: "test".into(), error_lines: vec![], context_lines: vec!["line".into()],
            log_url: "https://buildkite.com/x".into(), full_log_available: true,
        };
        let out = format_context_output(result);
        assert!(out.contains("--full-log"), "output should hint --full-log when full_log_available=true, got: {}", out);
    }

    // ADR-002 #6: format_context_output shows error_lines before context_lines
    #[test]
    fn format_context_output_shows_error_lines_before_context() {
        use crate::ci::{format_context_output, BuildkiteLogResult};
        let result = BuildkiteLogResult {
            step: "rspec".into(),
            error_lines: vec!["Error: test failed".into()],
            context_lines: vec!["context line 1".into(), "context line 2".into()],
            log_url: "https://buildkite.com/x/y/1".into(),
            full_log_available: true,
        };
        let out = format_context_output(result);
        let err_pos = out.find("Error: test failed").expect("error line must appear in output");
        let ctx_pos = out.find("context line 1").expect("context line must appear in output");
        assert!(err_pos < ctx_pos, "error_lines must appear before context_lines, positions: err={} ctx={}", err_pos, ctx_pos);
    }

    // ADR-002 #6: format_context_output always includes log_url
    #[test]
    fn format_context_output_includes_log_url() {
        use crate::ci::{format_context_output, BuildkiteLogResult};
        let result = BuildkiteLogResult {
            step: "build".into(),
            error_lines: vec![],
            context_lines: vec!["some output".into()],
            log_url: "https://buildkite.com/org/pipe/builds/42".into(),
            full_log_available: true,
        };
        let out = format_context_output(result);
        assert!(out.contains("https://buildkite.com/org/pipe/builds/42"),
            "output must contain log_url, got: {}", out);
    }

    // ADR-002 #6: format_context_output with empty error_lines omits errors section
    #[test]
    fn format_context_output_empty_error_lines_omits_errors_section() {
        use crate::ci::{format_context_output, BuildkiteLogResult};
        let result = BuildkiteLogResult {
            step: "build".into(),
            error_lines: vec![],
            context_lines: vec!["clean output".into()],
            log_url: "https://buildkite.com/x".into(),
            full_log_available: true,
        };
        let out = format_context_output(result);
        assert!(!out.contains("Errors:"), "empty error_lines should omit Errors: section, got: {}", out);
        assert!(out.contains("clean output"), "context lines must still appear, got: {}", out);
    }
}
