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
            .with_body(r#"{"number":7,"title":"t","draft":false,"head":{"ref":"b"},"user":{"login":"author"}}"#).create();
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
                "user": {"login": "reviewer"},
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

    // D3b: reply comment (in_reply_to_id set) is not a separate thread
    #[test]
    fn reply_comment_not_surfaced_as_separate_thread() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/repos/owner/repo/pulls/11")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"{"number":11,"title":"t","draft":false,"head":{"ref":"b"},"user":{"login":"author"}}"#).create();
        server.mock("GET", "/repos/owner/repo/commits/b/check-runs")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"{"check_runs":[]}"#).create();
        server.mock("GET", "/repos/owner/repo/branches/b/protection")
            .with_status(404).create();
        server.mock("GET", "/repos/owner/repo/pulls/11/reviews")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"[]"#).create();
        server.mock("GET", "/repos/owner/repo/pulls/11/comments")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"[
                {"id": 100, "body": "needs fix", "path": "src/lib.rs", "line": 10,
                 "user": {"login": "reviewer"}, "pull_request_review_id": 1},
                {"id": 101, "body": "fixed it", "path": "src/lib.rs", "line": 10,
                 "in_reply_to_id": 100,
                 "user": {"login": "author"}, "pull_request_review_id": 1}
            ]"#).create();

        let pr = mock_client(&server).fetch_pr("owner", "repo", 11).unwrap();
        assert_eq!(pr.threads.len(), 1, "reply comment should not be a separate thread");
        assert_eq!(pr.threads[0].id, 100, "thread ID should be root comment ID");
    }

    // D3c: thread state = Addressed when PR author's comment is last
    #[test]
    fn thread_addressed_when_author_replied_last() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/repos/owner/repo/pulls/12")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"{"number":12,"title":"t","draft":false,"head":{"ref":"b"},"user":{"login":"author"}}"#).create();
        server.mock("GET", "/repos/owner/repo/commits/b/check-runs")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"{"check_runs":[]}"#).create();
        server.mock("GET", "/repos/owner/repo/branches/b/protection")
            .with_status(404).create();
        server.mock("GET", "/repos/owner/repo/pulls/12/reviews")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"[]"#).create();
        server.mock("GET", "/repos/owner/repo/pulls/12/comments")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"[
                {"id": 200, "body": "needs fix", "path": "src/lib.rs", "line": 5,
                 "user": {"login": "reviewer"}, "pull_request_review_id": 2},
                {"id": 201, "body": "fixed", "path": "src/lib.rs", "line": 5,
                 "in_reply_to_id": 200,
                 "user": {"login": "author"}, "pull_request_review_id": 2}
            ]"#).create();

        let pr = mock_client(&server).fetch_pr("owner", "repo", 12).unwrap();
        assert_eq!(pr.threads.len(), 1);
        assert_eq!(pr.threads[0].state, ThreadState::Addressed,
            "thread should be Addressed when PR author replied last");
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

    // D5: check details_url populated from check-runs API
    #[test]
    fn check_details_url_populated() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/repos/owner/repo/pulls/10")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"{"number":10,"title":"t","draft":false,"head":{"ref":"b"}}"#).create();
        server.mock("GET", "/repos/owner/repo/commits/b/check-runs")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"{"check_runs":[{
                "name":"ci/test",
                "conclusion":"failure",
                "status":"completed",
                "details_url":"https://buildkite.com/org/pipeline/builds/123"
            }]}"#).create();
        server.mock("GET", "/repos/owner/repo/branches/b/protection")
            .with_status(404).create();
        server.mock("GET", "/repos/owner/repo/pulls/10/reviews")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"[]"#).create();
        server.mock("GET", "/repos/owner/repo/pulls/10/comments")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"[]"#).create();

        let pr = mock_client(&server).fetch_pr("owner", "repo", 10).unwrap();
        let check = pr.checks.iter().find(|c| c.name == "ci/test").unwrap();
        assert_eq!(check.details_url.as_deref(), Some("https://buildkite.com/org/pipeline/builds/123"));
    }

    // CR1: create_pr posts to correct endpoint and returns PR number
    #[test]
    fn create_pr_posts_and_returns_number() {
        let mut server = mockito::Server::new();
        server.mock("POST", "/repos/owner/repo/pulls")
            .with_status(201)
            .with_header("content-type", "application/json")
            .with_body(r#"{"number": 42, "html_url": "https://github.com/owner/repo/pull/42", "head": {"ref": "feat/thing"}, "title": "my feature"}"#)
            .create();

        let client = mock_client(&server);
        let pr = client.create_pr("owner", "repo", "my feature", "feat/thing", "main", true).unwrap();
        assert_eq!(pr.number, 42);
        assert_eq!(pr.title, "my feature");
        assert_eq!(pr.branch, "feat/thing");
    }

    // CR1: create_pr errors on API failure
    #[test]
    fn create_pr_errors_on_failure() {
        let mut server = mockito::Server::new();
        server.mock("POST", "/repos/owner/repo/pulls")
            .with_status(422)
            .with_header("content-type", "application/json")
            .with_body(r#"{"message": "Validation Failed"}"#)
            .create();

        let client = mock_client(&server);
        assert!(client.create_pr("owner", "repo", "title", "branch", "main", true).is_err());
    }

    // T1: reply_to_comment posts to correct endpoint (includes pr_number) and returns posted body
    #[test]
    fn reply_to_comment_posts_and_returns_body() {
        let mut server = mockito::Server::new();
        server.mock("POST", "/repos/owner/repo/pulls/42/comments/999/replies")
            .with_status(201)
            .with_header("content-type", "application/json")
            .with_body(r#"{"id": 1000, "body": "Thanks, fixed!"}"#)
            .create();

        let client = mock_client(&server);
        let body = client.reply_to_comment("owner", "repo", 42, 999, "Thanks, fixed!").unwrap();
        assert_eq!(body, "Thanks, fixed!");
    }

    // T1: reply_to_comment errors on API failure
    #[test]
    fn reply_to_comment_errors_on_failure() {
        let mut server = mockito::Server::new();
        server.mock("POST", "/repos/owner/repo/pulls/42/comments/999/replies")
            .with_status(422)
            .create();

        let client = mock_client(&server);
        assert!(client.reply_to_comment("owner", "repo", 42, 999, "text").is_err());
    }

    // D1: fetch_pr_metadata returns (title, branch) from GitHub API
    #[test]
    fn fetch_pr_metadata_returns_title_and_branch() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/repos/owner/repo/pulls/42")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"number":42,"title":"my feature","draft":false,"head":{"ref":"feat/thing"}}"#)
            .create();

        let client = mock_client(&server);
        let (title, branch) = client.fetch_pr_metadata("owner", "repo", 42).unwrap();
        assert_eq!(title, "my feature");
        assert_eq!(branch, "feat/thing");
    }

    // D2: fetch_pr_metadata returns error when API call fails
    #[test]
    fn fetch_pr_metadata_errors_on_404() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/repos/owner/repo/pulls/999")
            .with_status(404)
            .create();

        let client = mock_client(&server);
        assert!(client.fetch_pr_metadata("owner", "repo", 999).is_err());
    }
}
