#[cfg(test)]
mod tests {
    use crate::github::{GithubClient, parse_github_remote_pub};
    use crate::model::{CheckStatus, ThreadState};

    fn mock_client(server: &mockito::Server) -> GithubClient {
        GithubClient::with_base_url("test-token".into(), server.url())
    }

    // D1: PR metadata — title, branch, draft status
    #[test]
    fn fetch_pr_returns_title_branch_draft() {
        let mut server = mockito::Server::new();

        server.mock("GET", "/repos/owner/repo/pulls/42")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{
                "number": 42,
                "title": "my PR",
                "draft": false,
                "head": { "ref": "fix/foo" }
            }"#)
            .create();
        // checks endpoint
        server.mock("GET", "/repos/owner/repo/commits/fix%2Ffoo/check-runs")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"check_runs": []}"#)
            .create();
        // branch protection
        server.mock("GET", "/repos/owner/repo/branches/fix%2Ffoo/protection")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"required_status_checks": {"contexts": []}}"#)
            .create();
        // reviews
        server.mock("GET", "/repos/owner/repo/pulls/42/reviews")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[]"#)
            .create();
        // review comments
        server.mock("GET", "/repos/owner/repo/pulls/42/comments")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[]"#)
            .create();

        let client = mock_client(&server);
        let pr = client.fetch_pr("owner", "repo", 42).unwrap();
        assert_eq!(pr.title, "my PR");
        assert_eq!(pr.branch, "fix/foo");
        assert!(!pr.draft);
        assert_eq!(pr.number, 42);
    }

    // D1: draft flag
    #[test]
    fn fetch_pr_sets_draft_true() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/repos/owner/repo/pulls/1")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"number":1,"title":"draft","draft":true,"head":{"ref":"wip"}}"#)
            .create();
        server.mock("GET", "/repos/owner/repo/commits/wip/check-runs")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"{"check_runs":[]}"#).create();
        server.mock("GET", "/repos/owner/repo/branches/wip/protection")
            .with_status(404).create();
        server.mock("GET", "/repos/owner/repo/pulls/1/reviews")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"[]"#).create();
        server.mock("GET", "/repos/owner/repo/pulls/1/comments")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"[]"#).create();

        let pr = mock_client(&server).fetch_pr("owner", "repo", 1).unwrap();
        assert!(pr.draft);
    }

    // D2: checks populated; required flag set from branch protection
    #[test]
    fn checks_populated_with_required_flag() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/repos/owner/repo/pulls/5")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"{"number":5,"title":"t","draft":false,"head":{"ref":"mybranch"}}"#)
            .create();
        server.mock("GET", "/repos/owner/repo/commits/mybranch/check-runs")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"{"check_runs":[
                {"name":"ci/test","conclusion":"failure","status":"completed"},
                {"name":"ci/lint","conclusion":"success","status":"completed"}
            ]}"#).create();
        server.mock("GET", "/repos/owner/repo/branches/mybranch/protection")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"{"required_status_checks":{"contexts":["ci/test"]}}"#).create();
        server.mock("GET", "/repos/owner/repo/pulls/5/reviews")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"[]"#).create();
        server.mock("GET", "/repos/owner/repo/pulls/5/comments")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"[]"#).create();

        let pr = mock_client(&server).fetch_pr("owner", "repo", 5).unwrap();
        let test_check = pr.checks.iter().find(|c| c.name == "ci/test").unwrap();
        let lint_check = pr.checks.iter().find(|c| c.name == "ci/lint").unwrap();
        assert!(test_check.required);
        assert!(!lint_check.required);
        assert_eq!(test_check.status, CheckStatus::Fail);
        assert_eq!(lint_check.status, CheckStatus::Pass);
    }

    // D2: pending check maps to Pending status
    #[test]
    fn pending_check_maps_to_pending_status() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/repos/owner/repo/pulls/6")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"{"number":6,"title":"t","draft":false,"head":{"ref":"b"}}"#).create();
        server.mock("GET", "/repos/owner/repo/commits/b/check-runs")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"{"check_runs":[{"name":"ci/test","conclusion":null,"status":"in_progress"}]}"#).create();
        server.mock("GET", "/repos/owner/repo/branches/b/protection")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"{"required_status_checks":{"contexts":["ci/test"]}}"#).create();
        server.mock("GET", "/repos/owner/repo/pulls/6/reviews")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"[]"#).create();
        server.mock("GET", "/repos/owner/repo/pulls/6/comments")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"[]"#).create();

        let pr = mock_client(&server).fetch_pr("owner", "repo", 6).unwrap();
        let check = pr.checks.iter().find(|c| c.name == "ci/test").unwrap();
        assert_eq!(check.status, CheckStatus::Pending);
    }

    // D3: review threads populated with body, file, line, Open state
    #[test]
    fn review_comments_mapped_to_threads() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/repos/owner/repo/pulls/7")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"{"number":7,"title":"t","draft":false,"head":{"ref":"b"}}"#).create();
        server.mock("GET", "/repos/owner/repo/commits/b/check-runs")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"{"check_runs":[]}"#).create();
        server.mock("GET", "/repos/owner/repo/branches/b/protection")
            .with_status(404).create();
        server.mock("GET", "/repos/owner/repo/pulls/7/reviews")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"[]"#).create();
        server.mock("GET", "/repos/owner/repo/pulls/7/comments")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"[{
                "id": 999,
                "body": "needs fix",
                "path": "src/lib.rs",
                "line": 42,
                "pull_request_review_id": 1
            }]"#).create();

        let pr = mock_client(&server).fetch_pr("owner", "repo", 7).unwrap();
        assert_eq!(pr.threads.len(), 1);
        let t = &pr.threads[0];
        assert_eq!(t.id, 999);
        assert_eq!(t.body, "needs fix");
        assert_eq!(t.file.as_deref(), Some("src/lib.rs"));
        assert_eq!(t.line, Some(42));
        assert_eq!(t.state, ThreadState::Open);
    }

    // D4: approved true when APPROVED review present
    #[test]
    fn approved_true_when_approved_review_present() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/repos/owner/repo/pulls/8")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"{"number":8,"title":"t","draft":false,"head":{"ref":"b"}}"#).create();
        server.mock("GET", "/repos/owner/repo/commits/b/check-runs")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"{"check_runs":[]}"#).create();
        server.mock("GET", "/repos/owner/repo/branches/b/protection")
            .with_status(404).create();
        server.mock("GET", "/repos/owner/repo/pulls/8/reviews")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"[{"state":"APPROVED"}]"#).create();
        server.mock("GET", "/repos/owner/repo/pulls/8/comments")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"[]"#).create();

        let pr = mock_client(&server).fetch_pr("owner", "repo", 8).unwrap();
        assert!(pr.approved);
    }

    // D4: approved false when no APPROVED review
    #[test]
    fn approved_false_when_no_approved_review() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/repos/owner/repo/pulls/9")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"{"number":9,"title":"t","draft":false,"head":{"ref":"b"}}"#).create();
        server.mock("GET", "/repos/owner/repo/commits/b/check-runs")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"{"check_runs":[]}"#).create();
        server.mock("GET", "/repos/owner/repo/branches/b/protection")
            .with_status(404).create();
        server.mock("GET", "/repos/owner/repo/pulls/9/reviews")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"[{"state":"CHANGES_REQUESTED"}]"#).create();
        server.mock("GET", "/repos/owner/repo/pulls/9/comments")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"[]"#).create();

        let pr = mock_client(&server).fetch_pr("owner", "repo", 9).unwrap();
        assert!(!pr.approved);
    }

    // remote URL parsing
    #[test]
    fn parse_https_remote() {
        let result = parse_github_remote_pub("https://github.com/owner/repo.git");
        assert_eq!(result, Some(("owner".into(), "repo".into())));
    }

    #[test]
    fn parse_ssh_remote() {
        let result = parse_github_remote_pub("git@github.com:owner/repo.git");
        assert_eq!(result, Some(("owner".into(), "repo".into())));
    }
}
