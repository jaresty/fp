#[cfg(test)]
mod tests {
    use crate::github::{parse_github_remote_pub, GithubClient};
    use crate::model::{CheckStatus, ThreadState};

    fn mock_client(server: &mockito::Server) -> GithubClient {
        GithubClient::with_base_url("test-token".into(), server.url())
    }

    // D1: PR metadata — title, branch, draft status
    #[test]
    fn fetch_pr_returns_title_branch_draft() {
        let mut server = mockito::Server::new();

        server
            .mock("GET", "/repos/owner/repo/pulls/42")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "number": 42,
                "title": "my PR",
                "draft": false,
                "head": { "ref": "fix/foo" }
            }"#,
            )
            .create();
        // checks endpoint
        server
            .mock("GET", "/repos/owner/repo/commits/fix%2Ffoo/check-runs")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"check_runs": []}"#)
            .create();
        // branch protection
        server
            .mock("GET", "/repos/owner/repo/branches/fix%2Ffoo/protection")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"required_status_checks": {"contexts": []}}"#)
            .create();
        // reviews
        server
            .mock("GET", "/repos/owner/repo/pulls/42/reviews")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[]"#)
            .create();
        // review comments
        server
            .mock("GET", "/repos/owner/repo/pulls/42/comments?per_page=100&page=1")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[]"#)
            .create();
        server.mock("GET", "/repos/owner/repo/issues/42/comments?per_page=100&page=1")
            .with_status(200).with_header("content-type","application/json").with_body(r#"[]"#).create();

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
        server
            .mock("GET", "/repos/owner/repo/pulls/1")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"number":1,"title":"draft","draft":true,"head":{"ref":"wip"}}"#)
            .create();
        server
            .mock("GET", "/repos/owner/repo/commits/wip/check-runs")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"check_runs":[]}"#)
            .create();
        server
            .mock("GET", "/repos/owner/repo/branches/wip/protection")
            .with_status(404)
            .create();
        server
            .mock("GET", "/repos/owner/repo/pulls/1/reviews")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[]"#)
            .create();
        server
            .mock("GET", "/repos/owner/repo/pulls/1/comments?per_page=100&page=1")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[]"#)
            .create();
        server.mock("GET", "/repos/owner/repo/issues/1/comments?per_page=100&page=1")
            .with_status(200).with_header("content-type","application/json").with_body(r#"[]"#).create();

        let pr = mock_client(&server).fetch_pr("owner", "repo", 1).unwrap();
        assert!(pr.draft);
    }

    // D2: checks populated; required flag set from branch protection
    #[test]
    fn checks_populated_with_required_flag() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/repos/owner/repo/pulls/5")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"{"number":5,"title":"t","draft":false,"head":{"ref":"mybranch","sha":""},"base":{"ref":"main"}}"#)
            .create();
        server
            .mock("GET", "/repos/owner/repo/commits/mybranch/check-runs")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"check_runs":[
                {"name":"ci/test","conclusion":"failure","status":"completed"},
                {"name":"ci/lint","conclusion":"success","status":"completed"}
            ]}"#,
            )
            .create();
        server
            .mock("GET", "/repos/owner/repo/branches/mybranch/protection")
            .with_status(404)
            .create();
        server
            .mock("GET", "/repos/owner/repo/branches/main/protection")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"required_status_checks":{"contexts":["ci/test"]}}"#)
            .create();
        server
            .mock("GET", "/repos/owner/repo/commits//statuses")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[]"#)
            .create();
        server
            .mock("GET", "/repos/owner/repo/pulls/5/reviews")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[]"#)
            .create();
        server
            .mock("GET", "/repos/owner/repo/pulls/5/comments?per_page=100&page=1")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[]"#)
            .create();
        server.mock("GET", "/repos/owner/repo/issues/5/comments?per_page=100&page=1")
            .with_status(200).with_header("content-type","application/json").with_body(r#"[]"#).create();

        let pr = mock_client(&server).fetch_pr("owner", "repo", 5).unwrap();
        let test_check = pr.checks.iter().find(|c| c.name == "ci/test").unwrap();
        let lint_check = pr.checks.iter().find(|c| c.name == "ci/lint").unwrap();
        assert!(test_check.required);
        assert!(!lint_check.required);
        assert_eq!(test_check.status, CheckStatus::Fail);
        assert_eq!(lint_check.status, CheckStatus::Pass);
    }

    // D2b: skipped and neutral conclusions map to Pass (not Fail)
    #[test]
    fn skipped_and_neutral_conclusion_maps_to_pass() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/repos/owner/repo/pulls/55")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"{"number":55,"title":"t","draft":false,"head":{"ref":"br","sha":""},"base":{"ref":"main"}}"#)
            .create();
        server.mock("GET", "/repos/owner/repo/commits/br/check-runs")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"{"check_runs":[
                {"name":"skip-check","conclusion":"skipped","status":"completed"},
                {"name":"neutral-check","conclusion":"neutral","status":"completed"}
            ]}"#)
            .create();
        server.mock("GET", "/repos/owner/repo/branches/br/protection").with_status(404).create();
        server.mock("GET", "/repos/owner/repo/branches/main/protection").with_status(404).create();
        server.mock("GET", "/repos/owner/repo/commits//statuses")
            .with_status(200).with_header("content-type","application/json").with_body(r#"[]"#).create();
        server.mock("GET", "/repos/owner/repo/pulls/55/reviews")
            .with_status(200).with_header("content-type","application/json").with_body(r#"[]"#).create();
        server.mock("GET", "/repos/owner/repo/pulls/55/comments?per_page=100&page=1")
            .with_status(200).with_header("content-type","application/json").with_body(r#"[]"#).create();
        server.mock("GET", "/repos/owner/repo/issues/55/comments?per_page=100&page=1")
            .with_status(200).with_header("content-type","application/json").with_body(r#"[]"#).create();
        server.mock("GET", "/repos/owner/repo/pulls/55/requested_reviewers")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"{"users":[],"teams":[]}"#).create();

        let pr = mock_client(&server).fetch_pr("owner", "repo", 55).unwrap();
        let skip_check = pr.checks.iter().find(|c| c.name == "skip-check").unwrap();
        let neutral_check = pr.checks.iter().find(|c| c.name == "neutral-check").unwrap();
        assert_eq!(skip_check.status, CheckStatus::Pass, "skipped should map to Pass");
        assert_eq!(neutral_check.status, CheckStatus::Pass, "neutral should map to Pass");
    }

    // D2: pending check maps to Pending status
    #[test]
    fn pending_check_maps_to_pending_status() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", "/repos/owner/repo/pulls/6")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"number":6,"title":"t","draft":false,"head":{"ref":"b"}}"#)
            .create();
        server
            .mock("GET", "/repos/owner/repo/commits/b/check-runs")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"check_runs":[{"name":"ci/test","conclusion":null,"status":"in_progress"}]}"#,
            )
            .create();
        server
            .mock("GET", "/repos/owner/repo/branches/b/protection")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"required_status_checks":{"contexts":["ci/test"]}}"#)
            .create();
        server
            .mock("GET", "/repos/owner/repo/pulls/6/reviews")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[]"#)
            .create();
        server
            .mock("GET", "/repos/owner/repo/pulls/6/comments?per_page=100&page=1")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[]"#)
            .create();
        server.mock("GET", "/repos/owner/repo/issues/6/comments?per_page=100&page=1")
            .with_status(200).with_header("content-type","application/json").with_body(r#"[]"#).create();

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
        server
            .mock("GET", "/repos/owner/repo/commits/b/check-runs")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"check_runs":[]}"#)
            .create();
        server
            .mock("GET", "/repos/owner/repo/branches/b/protection")
            .with_status(404)
            .create();
        server
            .mock("GET", "/repos/owner/repo/pulls/7/reviews")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[]"#)
            .create();
        server
            .mock("GET", "/repos/owner/repo/pulls/7/comments?per_page=100&page=1")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"[{
                "id": 999,
                "body": "needs fix",
                "path": "src/lib.rs",
                "line": 42,
                "user": {"login": "reviewer"},
                "pull_request_review_id": 1
            }]"#,
            )
            .create();
        server.mock("GET", "/repos/owner/repo/issues/7/comments?per_page=100&page=1")
            .with_status(200).with_header("content-type","application/json").with_body(r#"[]"#).create();

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
        server
            .mock("GET", "/repos/owner/repo/commits/b/check-runs")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"check_runs":[]}"#)
            .create();
        server
            .mock("GET", "/repos/owner/repo/branches/b/protection")
            .with_status(404)
            .create();
        server
            .mock("GET", "/repos/owner/repo/pulls/11/reviews")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[]"#)
            .create();
        server
            .mock("GET", "/repos/owner/repo/pulls/11/comments?per_page=100&page=1")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"[
                {"id": 100, "body": "needs fix", "path": "src/lib.rs", "line": 10,
                 "user": {"login": "reviewer"}, "pull_request_review_id": 1},
                {"id": 101, "body": "fixed it", "path": "src/lib.rs", "line": 10,
                 "in_reply_to_id": 100,
                 "user": {"login": "author"}, "pull_request_review_id": 1}
            ]"#,
            )
            .create();
        server.mock("GET", "/repos/owner/repo/issues/11/comments?per_page=100&page=1")
            .with_status(200).with_header("content-type","application/json").with_body(r#"[]"#).create();

        let pr = mock_client(&server).fetch_pr("owner", "repo", 11).unwrap();
        assert_eq!(
            pr.threads.len(),
            1,
            "reply comment should not be a separate thread"
        );
        assert_eq!(pr.threads[0].id, 100, "thread ID should be root comment ID");
    }

    // D3e: thread.author is the root commenter; replies carry (author, body) tuples
    #[test]
    fn thread_author_and_reply_authors_populated() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/repos/owner/repo/pulls/88")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"{"number":88,"title":"t","draft":false,"head":{"ref":"b"},"user":{"login":"pr-author"}}"#).create();
        server.mock("GET", "/repos/owner/repo/commits/b/check-runs")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"{"check_runs":[]}"#).create();
        server.mock("GET", "/repos/owner/repo/branches/b/protection")
            .with_status(404).create();
        server.mock("GET", "/repos/owner/repo/pulls/88/reviews")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"[]"#).create();
        server.mock("GET", "/repos/owner/repo/pulls/88/comments?per_page=100&page=1")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"[
                {"id": 300, "body": "root comment", "path": "src/lib.rs", "line": 5,
                 "user": {"login": "reviewer"}},
                {"id": 301, "body": "reply one", "path": "src/lib.rs", "line": 5,
                 "in_reply_to_id": 300, "user": {"login": "pr-author"}},
                {"id": 302, "body": "reply two", "path": "src/lib.rs", "line": 5,
                 "in_reply_to_id": 300, "user": {"login": "reviewer"}}
            ]"#).create();
        server.mock("GET", "/repos/owner/repo/issues/88/comments?per_page=100&page=1")
            .with_status(200).with_header("content-type","application/json").with_body(r#"[]"#).create();

        let pr = mock_client(&server).fetch_pr("owner", "repo", 88).unwrap();
        assert_eq!(pr.threads.len(), 1);
        let t = &pr.threads[0];
        assert_eq!(t.author, "reviewer", "root author should be 'reviewer'");
        assert_eq!(t.replies.len(), 2);
        assert_eq!(t.replies[0].0, "pr-author", "first reply author should be 'pr-author'");
        assert_eq!(t.replies[0].1, "reply one");
        assert_eq!(t.replies[1].0, "reviewer");
        assert_eq!(t.replies[1].1, "reply two");
    }

    // D3d: thread.replies contains bodies of non-root comments in order
    #[test]
    fn thread_replies_populated_from_non_root_comments() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/repos/owner/repo/pulls/77")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"{"number":77,"title":"t","draft":false,"head":{"ref":"b"},"user":{"login":"author"}}"#).create();
        server.mock("GET", "/repos/owner/repo/commits/b/check-runs")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"{"check_runs":[]}"#).create();
        server.mock("GET", "/repos/owner/repo/branches/b/protection")
            .with_status(404).create();
        server.mock("GET", "/repos/owner/repo/pulls/77/reviews")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"[]"#).create();
        server.mock("GET", "/repos/owner/repo/pulls/77/comments?per_page=100&page=1")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"[
                {"id": 200, "body": "root comment", "path": "src/lib.rs", "line": 5,
                 "user": {"login": "reviewer"}},
                {"id": 201, "body": "first reply", "path": "src/lib.rs", "line": 5,
                 "in_reply_to_id": 200, "user": {"login": "author"}},
                {"id": 202, "body": "second reply", "path": "src/lib.rs", "line": 5,
                 "in_reply_to_id": 200, "user": {"login": "reviewer"}}
            ]"#).create();
        server.mock("GET", "/repos/owner/repo/issues/77/comments?per_page=100&page=1")
            .with_status(200).with_header("content-type","application/json").with_body(r#"[]"#).create();

        let pr = mock_client(&server).fetch_pr("owner", "repo", 77).unwrap();
        assert_eq!(pr.threads.len(), 1);
        let t = &pr.threads[0];
        assert_eq!(t.body, "root comment");
        assert_eq!(t.replies.len(), 2, "thread should have 2 replies, got: {:?}", t.replies);
        assert_eq!(t.replies[0].1, "first reply");
        assert_eq!(t.replies[1].1, "second reply");
    }

    // D3c: thread state = Addressed when PR author's comment is last
    #[test]
    fn thread_addressed_when_author_replied_last() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/repos/owner/repo/pulls/12")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"{"number":12,"title":"t","draft":false,"head":{"ref":"b"},"user":{"login":"author"}}"#).create();
        server
            .mock("GET", "/repos/owner/repo/commits/b/check-runs")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"check_runs":[]}"#)
            .create();
        server
            .mock("GET", "/repos/owner/repo/branches/b/protection")
            .with_status(404)
            .create();
        server
            .mock("GET", "/repos/owner/repo/pulls/12/reviews")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[]"#)
            .create();
        server
            .mock("GET", "/repos/owner/repo/pulls/12/comments?per_page=100&page=1")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"[
                {"id": 200, "body": "needs fix", "path": "src/lib.rs", "line": 5,
                 "user": {"login": "reviewer"}, "pull_request_review_id": 2},
                {"id": 201, "body": "fixed", "path": "src/lib.rs", "line": 5,
                 "in_reply_to_id": 200,
                 "user": {"login": "author"}, "pull_request_review_id": 2}
            ]"#,
            )
            .create();
        server.mock("GET", "/repos/owner/repo/issues/12/comments?per_page=100&page=1")
            .with_status(200).with_header("content-type","application/json").with_body(r#"[]"#).create();

        let pr = mock_client(&server).fetch_pr("owner", "repo", 12).unwrap();
        assert_eq!(pr.threads.len(), 1);
        assert_eq!(
            pr.threads[0].state,
            ThreadState::Addressed,
            "thread should be Addressed when PR author replied last"
        );
    }

    // D4: approved true when APPROVED review present
    #[test]
    fn approved_true_when_approved_review_present() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", "/repos/owner/repo/pulls/8")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"number":8,"title":"t","draft":false,"head":{"ref":"b"}}"#)
            .create();
        server
            .mock("GET", "/repos/owner/repo/commits/b/check-runs")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"check_runs":[]}"#)
            .create();
        server
            .mock("GET", "/repos/owner/repo/branches/b/protection")
            .with_status(404)
            .create();
        server
            .mock("GET", "/repos/owner/repo/pulls/8/reviews")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[{"state":"APPROVED"}]"#)
            .create();
        server
            .mock("GET", "/repos/owner/repo/pulls/8/comments?per_page=100&page=1")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[]"#)
            .create();
        server.mock("GET", "/repos/owner/repo/issues/8/comments?per_page=100&page=1")
            .with_status(200).with_header("content-type","application/json").with_body(r#"[]"#).create();

        let pr = mock_client(&server).fetch_pr("owner", "repo", 8).unwrap();
        assert!(pr.approved);
    }

    // D4: approved false when no APPROVED review
    #[test]
    fn approved_false_when_no_approved_review() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", "/repos/owner/repo/pulls/9")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"number":9,"title":"t","draft":false,"head":{"ref":"b"}}"#)
            .create();
        server
            .mock("GET", "/repos/owner/repo/commits/b/check-runs")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"check_runs":[]}"#)
            .create();
        server
            .mock("GET", "/repos/owner/repo/branches/b/protection")
            .with_status(404)
            .create();
        server
            .mock("GET", "/repos/owner/repo/pulls/9/reviews")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[{"state":"CHANGES_REQUESTED"}]"#)
            .create();
        server
            .mock("GET", "/repos/owner/repo/pulls/9/comments?per_page=100&page=1")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[]"#)
            .create();
        server.mock("GET", "/repos/owner/repo/issues/9/comments?per_page=100&page=1")
            .with_status(200).with_header("content-type","application/json").with_body(r#"[]"#).create();

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
        server
            .mock("GET", "/repos/owner/repo/pulls/10")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"number":10,"title":"t","draft":false,"head":{"ref":"b"}}"#)
            .create();
        server
            .mock("GET", "/repos/owner/repo/commits/b/check-runs")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"check_runs":[{
                "name":"ci/test",
                "conclusion":"failure",
                "status":"completed",
                "details_url":"https://buildkite.com/org/pipeline/builds/123"
            }]}"#,
            )
            .create();
        server
            .mock("GET", "/repos/owner/repo/branches/b/protection")
            .with_status(404)
            .create();
        server
            .mock("GET", "/repos/owner/repo/pulls/10/reviews")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[]"#)
            .create();
        server
            .mock("GET", "/repos/owner/repo/pulls/10/comments?per_page=100&page=1")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[]"#)
            .create();
        server.mock("GET", "/repos/owner/repo/issues/10/comments?per_page=100&page=1")
            .with_status(200).with_header("content-type","application/json").with_body(r#"[]"#).create();

        let pr = mock_client(&server).fetch_pr("owner", "repo", 10).unwrap();
        let check = pr.checks.iter().find(|c| c.name == "ci/test").unwrap();
        assert_eq!(
            check.details_url.as_deref(),
            Some("https://buildkite.com/org/pipeline/builds/123")
        );
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
        let pr = client
            .create_pr_with_body("owner", "repo", "my feature", "feat/thing", "main", true, None)
            .unwrap();
        assert_eq!(pr.number, 42);
        assert_eq!(pr.title, "my feature");
        assert_eq!(pr.branch, "feat/thing");
    }

    // CR1: create_pr errors on API failure
    #[test]
    fn create_pr_errors_on_failure() {
        let mut server = mockito::Server::new();
        server
            .mock("POST", "/repos/owner/repo/pulls")
            .with_status(422)
            .with_header("content-type", "application/json")
            .with_body(r#"{"message": "Validation Failed"}"#)
            .create();

        let client = mock_client(&server);
        assert!(client
            .create_pr_with_body("owner", "repo", "title", "branch", "main", true, None)
            .is_err());
    }

    // T1: reply_to_comment posts to correct endpoint (includes pr_number) and returns posted body
    #[test]
    fn reply_to_comment_posts_and_returns_body() {
        let mut server = mockito::Server::new();
        server
            .mock("POST", "/repos/owner/repo/pulls/42/comments/999/replies")
            .with_status(201)
            .with_header("content-type", "application/json")
            .with_body(r#"{"id": 1000, "body": "Thanks, fixed!"}"#)
            .create();

        let client = mock_client(&server);
        let body = client
            .reply_to_comment("owner", "repo", 42, 999, "Thanks, fixed!")
            .unwrap();
        assert_eq!(body, "Thanks, fixed!");
    }

    // RT3: reply_to_thread prepends @author when routing to issues API (file is None)
    #[test]
    fn reply_to_thread_prepends_mention_for_pr_level_thread() {
        use crate::model::{Thread, ThreadState};
        let mut server = mockito::Server::new();
        // Expect the body to contain "@reviewer " prefix
        server.mock("POST", "/repos/owner/repo/issues/42/comments")
            .match_body(mockito::Matcher::PartialJson(serde_json::json!({"body": "@reviewer acknowledged"})))
            .with_status(201).with_header("content-type","application/json")
            .with_body(r#"{"id":2000,"html_url":"https://github.com/owner/repo/issues/42#issuecomment-2000"}"#).create();

        let thread = Thread {
            id: 500, state: ThreadState::Open, author: "reviewer".into(),
            body: "general comment".into(), replies: vec![],
            file: None, line: None,
        };
        let client = mock_client(&server);
        let result = client.reply_to_thread("owner", "repo", 42, &thread, "acknowledged");
        assert!(result.is_ok(), "expected Ok with @mention, got: {:?}", result);
    }

    // RV6: review body thread is Addressed when PR author has issue comment after review submitted_at
    #[test]
    fn review_body_thread_addressed_when_author_replied_after() {
        let mut server = mockito::Server::new();
        // Review submitted at T1, author replies at T2 > T1
        let reviews = r#"[{"id":200,"state":"CHANGES_REQUESTED","body":"Fix naming","user":{"login":"reviewer","type":"User"},"submitted_at":"2024-01-01T10:00:00Z"}]"#;
        // Issue comment from author at T2 > T1
        let issue_comments = r#"[{"id":300,"body":"Fixed","user":{"login":"author","type":"User"},"created_at":"2024-01-01T11:00:00Z"}]"#;
        // Use a custom mock setup with specific reviews and issue comments
        let pr_body = r#"{"number":95,"title":"t","draft":false,"head":{"ref":"b","sha":"sha95"},"base":{"ref":"main"},"user":{"login":"author"}}"#;
        server.mock("GET", "/repos/owner/repo/pulls/95").with_status(200).with_header("content-type","application/json").with_body(pr_body).create();
        server.mock("GET", "/repos/owner/repo/commits/b/check-runs").with_status(200).with_header("content-type","application/json").with_body(r#"{"check_runs":[]}"#).create();
        server.mock("GET", "/repos/owner/repo/branches/main/protection").with_status(404).create();
        server.mock("GET", "/repos/owner/repo/commits/sha95/statuses").with_status(200).with_header("content-type","application/json").with_body(r#"[]"#).create();
        server.mock("GET", "/repos/owner/repo/pulls/95/reviews").with_status(200).with_header("content-type","application/json").with_body(reviews).create();
        server.mock("GET", "/repos/owner/repo/pulls/95/requested_reviewers").with_status(200).with_header("content-type","application/json").with_body(r#"{"users":[],"teams":[]}"#).create();
        server.mock("GET", "/repos/owner/repo/pulls/95/comments?per_page=100&page=1").with_status(200).with_header("content-type","application/json").with_body(r#"[]"#).create();
        server.mock("GET", "/repos/owner/repo/issues/95/comments?per_page=100&page=1").with_status(200).with_header("content-type","application/json").with_body(issue_comments).create();

        let pr = mock_client(&server).fetch_pr("owner", "repo", 95).unwrap();
        let review_threads: Vec<_> = pr.threads.iter().filter(|t| t.file.is_none()).collect();
        assert_eq!(review_threads.len(), 1, "expected 1 review thread");
        assert_eq!(review_threads[0].state, crate::model::ThreadState::Addressed,
            "expected Addressed when author replied after review, got {:?}", review_threads[0].state);
    }

    // RV7: review body thread remains Open when no author issue comment exists after review
    #[test]
    fn review_body_thread_open_when_no_author_reply() {
        let mut server = mockito::Server::new();
        let reviews = r#"[{"id":201,"state":"CHANGES_REQUESTED","body":"Fix naming","user":{"login":"reviewer","type":"User"},"submitted_at":"2024-01-01T10:00:00Z"}]"#;
        // No issue comments at all
        let pr_body = r#"{"number":96,"title":"t","draft":false,"head":{"ref":"b","sha":"sha96"},"base":{"ref":"main"},"user":{"login":"author"}}"#;
        server.mock("GET", "/repos/owner/repo/pulls/96").with_status(200).with_header("content-type","application/json").with_body(pr_body).create();
        server.mock("GET", "/repos/owner/repo/commits/b/check-runs").with_status(200).with_header("content-type","application/json").with_body(r#"{"check_runs":[]}"#).create();
        server.mock("GET", "/repos/owner/repo/branches/main/protection").with_status(404).create();
        server.mock("GET", "/repos/owner/repo/commits/sha96/statuses").with_status(200).with_header("content-type","application/json").with_body(r#"[]"#).create();
        server.mock("GET", "/repos/owner/repo/pulls/96/reviews").with_status(200).with_header("content-type","application/json").with_body(reviews).create();
        server.mock("GET", "/repos/owner/repo/pulls/96/requested_reviewers").with_status(200).with_header("content-type","application/json").with_body(r#"{"users":[],"teams":[]}"#).create();
        server.mock("GET", "/repos/owner/repo/pulls/96/comments?per_page=100&page=1").with_status(200).with_header("content-type","application/json").with_body(r#"[]"#).create();
        server.mock("GET", "/repos/owner/repo/issues/96/comments?per_page=100&page=1").with_status(200).with_header("content-type","application/json").with_body(r#"[]"#).create();

        let pr = mock_client(&server).fetch_pr("owner", "repo", 96).unwrap();
        let review_threads: Vec<_> = pr.threads.iter().filter(|t| t.file.is_none()).collect();
        assert_eq!(review_threads.len(), 1, "expected 1 review thread");
        assert_eq!(review_threads[0].state, crate::model::ThreadState::Open,
            "expected Open when no author reply, got {:?}", review_threads[0].state);
    }

    // RV8: review body thread is Open when reviewer replies after author's acknowledgement (interleaved)
    #[test]
    fn review_body_thread_open_when_reviewer_replies_after_author() {
        let mut server = mockito::Server::new();
        // Review at T1, author replies at T2, reviewer comes back at T3 — should be Open
        let reviews = r#"[{"id":202,"state":"CHANGES_REQUESTED","body":"Fix naming","user":{"login":"reviewer","type":"User"},"submitted_at":"2024-01-01T10:00:00Z"}]"#;
        let issue_comments = r#"[
            {"id":301,"body":"Fixed","user":{"login":"author","type":"User"},"created_at":"2024-01-01T11:00:00Z"},
            {"id":302,"body":"Still not quite right","user":{"login":"reviewer","type":"User"},"created_at":"2024-01-01T12:00:00Z"}
        ]"#;
        let pr_body = r#"{"number":97,"title":"t","draft":false,"head":{"ref":"b","sha":"sha97"},"base":{"ref":"main"},"user":{"login":"author"}}"#;
        server.mock("GET", "/repos/owner/repo/pulls/97").with_status(200).with_header("content-type","application/json").with_body(pr_body).create();
        server.mock("GET", "/repos/owner/repo/commits/b/check-runs").with_status(200).with_header("content-type","application/json").with_body(r#"{"check_runs":[]}"#).create();
        server.mock("GET", "/repos/owner/repo/branches/main/protection").with_status(404).create();
        server.mock("GET", "/repos/owner/repo/commits/sha97/statuses").with_status(200).with_header("content-type","application/json").with_body(r#"[]"#).create();
        server.mock("GET", "/repos/owner/repo/pulls/97/reviews").with_status(200).with_header("content-type","application/json").with_body(reviews).create();
        server.mock("GET", "/repos/owner/repo/pulls/97/requested_reviewers").with_status(200).with_header("content-type","application/json").with_body(r#"{"users":[],"teams":[]}"#).create();
        server.mock("GET", "/repos/owner/repo/pulls/97/comments?per_page=100&page=1").with_status(200).with_header("content-type","application/json").with_body(r#"[]"#).create();
        server.mock("GET", "/repos/owner/repo/issues/97/comments?per_page=100&page=1").with_status(200).with_header("content-type","application/json").with_body(issue_comments).create();

        let pr = mock_client(&server).fetch_pr("owner", "repo", 97).unwrap();
        let review_threads: Vec<_> = pr.threads.iter().filter(|t| t.file.is_none()).collect();
        // The reviewer's T3 comment surfaces as a separate issue thread AND the review body thread should be Open
        let review_body = review_threads.iter().find(|t| t.id == 202);
        assert!(review_body.is_some(), "expected review body thread with id=202");
        assert_eq!(review_body.unwrap().state, crate::model::ThreadState::Open,
            "expected Open when reviewer replied after author, got {:?}", review_body.unwrap().state);
    }

    // RT1: reply_to_thread routes to pulls/comments/replies for inline thread (file is Some)
    #[test]
    fn reply_to_thread_uses_pulls_api_for_inline_thread() {
        use crate::model::{Thread, ThreadState};
        let mut server = mockito::Server::new();
        server.mock("POST", "/repos/owner/repo/pulls/42/comments/999/replies")
            .with_status(201).with_header("content-type","application/json")
            .with_body(r#"{"id":1000,"body":"done"}"#).create();

        let thread = Thread {
            id: 999, state: ThreadState::Open, author: "rev".into(),
            body: "fix this".into(), replies: vec![],
            file: Some("src/main.rs".into()), line: Some(10),
        };
        let client = mock_client(&server);
        let result = client.reply_to_thread("owner", "repo", 42, &thread, "done");
        assert!(result.is_ok(), "expected Ok for inline thread reply, got: {:?}", result);
    }

    // RT2: reply_to_thread routes to issues/comments for PR-level thread (file is None)
    #[test]
    fn reply_to_thread_uses_issues_api_for_pr_level_thread() {
        use crate::model::{Thread, ThreadState};
        let mut server = mockito::Server::new();
        server.mock("POST", "/repos/owner/repo/issues/42/comments")
            .with_status(201).with_header("content-type","application/json")
            .with_body(r#"{"id":2000,"html_url":"https://github.com/owner/repo/issues/42#issuecomment-2000"}"#).create();

        let thread = Thread {
            id: 500, state: ThreadState::Open, author: "rev".into(),
            body: "general comment".into(), replies: vec![],
            file: None, line: None,
        };
        let client = mock_client(&server);
        let result = client.reply_to_thread("owner", "repo", 42, &thread, "acknowledged");
        assert!(result.is_ok(), "expected Ok for PR-level thread reply, got: {:?}", result);
    }

    // T1: reply_to_comment errors on API failure
    #[test]
    fn reply_to_comment_errors_on_failure() {
        let mut server = mockito::Server::new();
        server
            .mock("POST", "/repos/owner/repo/pulls/42/comments/999/replies")
            .with_status(422)
            .create();

        let client = mock_client(&server);
        assert!(client
            .reply_to_comment("owner", "repo", 42, 999, "text")
            .is_err());
    }

    // D1: fetch_pr_metadata returns (title, branch) from GitHub API
    #[test]
    fn fetch_pr_metadata_returns_title_and_branch() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", "/repos/owner/repo/pulls/42")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"number":42,"title":"my feature","draft":false,"head":{"ref":"feat/thing"}}"#,
            )
            .create();

        let client = mock_client(&server);
        let (title, branch) = client.fetch_pr_metadata("owner", "repo", 42).unwrap();
        assert_eq!(title, "my feature");
        assert_eq!(branch, "feat/thing");
    }

    // BP1: required check names come from BASE branch protection, not head branch
    #[test]
    fn required_names_from_base_branch_not_head() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/repos/owner/repo/pulls/30")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"{"number":30,"title":"t","draft":false,"head":{"ref":"feat/x","sha":"sha1"},"base":{"ref":"main"},"user":{"login":"author"}}"#)
            .create();
        server.mock("GET", "/repos/owner/repo/commits/feat%2Fx/check-runs")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"{"check_runs":[{"name":"ci/test","conclusion":"failure","status":"completed"}]}"#)
            .create();
        // HEAD branch has no protection (feature branch)
        server
            .mock("GET", "/repos/owner/repo/branches/feat%2Fx/protection")
            .with_status(404)
            .create();
        // BASE branch (main) has the required checks configured
        server
            .mock("GET", "/repos/owner/repo/branches/main/protection")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"required_status_checks":{"contexts":["ci/test"]}}"#)
            .create();
        server
            .mock("GET", "/repos/owner/repo/commits/sha1/statuses")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[]"#)
            .create();
        server
            .mock("GET", "/repos/owner/repo/pulls/30/reviews")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[]"#)
            .create();
        server
            .mock("GET", "/repos/owner/repo/pulls/30/comments?per_page=100&page=1")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[]"#)
            .create();
        server.mock("GET", "/repos/owner/repo/issues/30/comments?per_page=100&page=1")
            .with_status(200).with_header("content-type","application/json").with_body(r#"[]"#).create();

        let pr = mock_client(&server).fetch_pr("owner", "repo", 30).unwrap();
        let check = pr.checks.iter().find(|c| c.name == "ci/test").unwrap();
        assert!(
            check.required,
            "ci/test should be required per base branch (main) protection"
        );
    }

    // CS1: commit statuses (Buildkite-style) are included in checks alongside check-runs
    #[test]
    fn commit_statuses_merged_into_checks() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/repos/owner/repo/pulls/20")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"{"number":20,"title":"t","draft":false,"head":{"ref":"b","sha":"abc123"},"base":{"ref":"main"},"user":{"login":"author"}}"#)
            .create();
        server
            .mock("GET", "/repos/owner/repo/commits/b/check-runs")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"check_runs":[{"name":"lint","conclusion":"success","status":"completed"}]}"#,
            )
            .create();
        server
            .mock("GET", "/repos/owner/repo/branches/b/protection")
            .with_status(404)
            .create();
        server
            .mock("GET", "/repos/owner/repo/branches/main/protection")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"required_status_checks":{"contexts":["buildkite/ci"]}}"#)
            .create();
        server.mock("GET", "/repos/owner/repo/commits/abc123/statuses")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"[
                {"context":"buildkite/ci","state":"failure","target_url":"https://buildkite.com/build/1"},
                {"context":"buildkite/lint","state":"success","target_url":null}
            ]"#)
            .create();
        server
            .mock("GET", "/repos/owner/repo/pulls/20/reviews")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[]"#)
            .create();
        server
            .mock("GET", "/repos/owner/repo/pulls/20/comments?per_page=100&page=1")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[]"#)
            .create();
        server.mock("GET", "/repos/owner/repo/issues/20/comments?per_page=100&page=1")
            .with_status(200).with_header("content-type","application/json").with_body(r#"[]"#).create();

        let pr = mock_client(&server).fetch_pr("owner", "repo", 20).unwrap();
        // check-run still present
        assert!(
            pr.checks.iter().any(|c| c.name == "lint"),
            "check-run check missing"
        );
        // commit status present
        let bk = pr
            .checks
            .iter()
            .find(|c| c.name == "buildkite/ci")
            .expect("buildkite/ci status missing from checks");
        assert_eq!(
            bk.status,
            CheckStatus::Fail,
            "failure state should map to Fail"
        );
        assert!(
            bk.required,
            "buildkite/ci should be required per branch protection"
        );
        assert_eq!(
            bk.details_url.as_deref(),
            Some("https://buildkite.com/build/1")
        );
        let bk_lint = pr
            .checks
            .iter()
            .find(|c| c.name == "buildkite/lint")
            .expect("buildkite/lint status missing");
        assert_eq!(bk_lint.status, CheckStatus::Pass);
        assert!(!bk_lint.required);
    }

    // B1: fetch_pr populates base field from base.ref in API response
    #[test]
    fn fetch_pr_populates_base_field() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/repos/owner/repo/pulls/55")
            .with_status(200).with_header("content-type", "application/json")
            .with_body(r#"{"number":55,"title":"t","draft":false,"head":{"ref":"feat/x","sha":"abc"},"base":{"ref":"develop"},"user":{"login":"author"}}"#)
            .create();
        server.mock("GET", "/repos/owner/repo/commits/feat%2Fx/check-runs")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"{"check_runs":[]}"#).create();
        server.mock("GET", "/repos/owner/repo/branches/develop/protection")
            .with_status(404).create();
        server.mock("GET", "/repos/owner/repo/commits/abc/statuses")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"[]"#).create();
        server.mock("GET", "/repos/owner/repo/pulls/55/reviews")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"[]"#).create();
        server.mock("GET", "/repos/owner/repo/pulls/55/requested_reviewers")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"{"users":[],"teams":[]}"#).create();
        server.mock("GET", "/repos/owner/repo/pulls/55/comments?per_page=100&page=1")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"[]"#).create();
        server.mock("GET", "/repos/owner/repo/issues/55/comments?per_page=100&page=1")
            .with_status(200).with_header("content-type","application/json").with_body(r#"[]"#).create();

        let pr = mock_client(&server).fetch_pr("owner", "repo", 55).unwrap();
        assert_eq!(pr.base, "develop", "expected base to be 'develop', got '{}'", pr.base);
    }

    // D2: fetch_pr_metadata returns error when API call fails
    #[test]
    fn fetch_pr_metadata_errors_on_404() {
        let mut server = mockito::Server::new();
        server
            .mock("GET", "/repos/owner/repo/pulls/999")
            .with_status(404)
            .create();

        let client = mock_client(&server);
        assert!(client.fetch_pr_metadata("owner", "repo", 999).is_err());
    }

    // Pagination: fetch_pr uses per_page=100 to get all review comments
    #[test]
    fn fetch_pr_includes_per_page_param_for_comments() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/repos/owner/repo/pulls/42")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"{"number":42,"title":"t","draft":false,"head":{"ref":"b"},"user":{"login":"author"}}"#)
            .create();
        server
            .mock("GET", "/repos/owner/repo/commits/b/check-runs")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"check_runs":[]}"#)
            .create();
        server
            .mock("GET", "/repos/owner/repo/branches/b/protection")
            .with_status(404)
            .create();
        server
            .mock("GET", "/repos/owner/repo/pulls/42/reviews")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[]"#)
            .create();
        // The fix requires per_page=100&page=1 (paginated path)
        server
            .mock("GET", "/repos/owner/repo/pulls/42/comments?per_page=100&page=1")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[]"#)
            .create();
        server.mock("GET", "/repos/owner/repo/issues/42/comments?per_page=100&page=1")
            .with_status(200).with_header("content-type","application/json").with_body(r#"[]"#).create();

        let pr = mock_client(&server).fetch_pr("owner", "repo", 42).unwrap();
        assert_eq!(pr.threads.len(), 0);
    }

    fn base_pr_mocks(server: &mut mockito::Server, pr_number: u64, comments_path: &str, comments_body: &str) {
        server.mock("GET", format!("/repos/owner/repo/pulls/{}", pr_number).as_str())
            .with_status(200).with_header("content-type", "application/json")
            .with_body(format!(r#"{{"number":{},"title":"t","draft":false,"head":{{"ref":"b"}},"user":{{"login":"author"}}}}"#, pr_number))
            .create();
        server.mock("GET", format!("/repos/owner/repo/commits/b/check-runs").as_str())
            .with_status(200).with_header("content-type", "application/json")
            .with_body(r#"{"check_runs":[]}"#).create();
        server.mock("GET", "/repos/owner/repo/branches/b/protection")
            .with_status(404).create();
        server.mock("GET", format!("/repos/owner/repo/pulls/{}/reviews", pr_number).as_str())
            .with_status(200).with_header("content-type", "application/json")
            .with_body(r#"[]"#).create();
        server.mock("GET", comments_path)
            .with_status(200).with_header("content-type", "application/json")
            .with_body(comments_body).create();
    }

    // D1+D2: get_paginated follows Link header and accumulates items across pages, stopping when no next
    #[test]
    fn fetch_pr_paginates_comments_across_multiple_pages() {
        let mut server = mockito::Server::new();
        let page2_url = format!("{}/repos/owner/repo/pulls/43/comments?per_page=100&page=2", server.url());
        let link_header = format!(r#"<{}>; rel="next""#, page2_url);

        server.mock("GET", "/repos/owner/repo/pulls/43")
            .with_status(200).with_header("content-type", "application/json")
            .with_body(r#"{"number":43,"title":"t","draft":false,"head":{"ref":"b"},"user":{"login":"author"}}"#)
            .create();
        server.mock("GET", "/repos/owner/repo/commits/b/check-runs")
            .with_status(200).with_header("content-type", "application/json")
            .with_body(r#"{"check_runs":[]}"#).create();
        server.mock("GET", "/repos/owner/repo/branches/b/protection")
            .with_status(404).create();
        server.mock("GET", "/repos/owner/repo/pulls/43/reviews")
            .with_status(200).with_header("content-type", "application/json")
            .with_body(r#"[]"#).create();

        // Page 1: returns one comment, has Link: next
        server.mock("GET", "/repos/owner/repo/pulls/43/comments?per_page=100&page=1")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_header("link", &link_header)
            .with_body(r#"[{"id":1,"path":"a.rs","line":1,"body":"comment1","user":{"login":"reviewer"},"in_reply_to_id":null}]"#)
            .create();

        // Page 2: returns one comment, no Link header
        server.mock("GET", "/repos/owner/repo/pulls/43/comments?per_page=100&page=2")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[{"id":2,"path":"b.rs","line":2,"body":"comment2","user":{"login":"reviewer"},"in_reply_to_id":null}]"#)
            .create();
        server.mock("GET", "/repos/owner/repo/issues/43/comments?per_page=100&page=1")
            .with_status(200).with_header("content-type","application/json").with_body(r#"[]"#).create();

        let pr = mock_client(&server).fetch_pr("owner", "repo", 43).unwrap();
        // Both comments from both pages must be present
        assert_eq!(pr.threads.len(), 2, "expected threads from both pages, got {}", pr.threads.len());
    }

    // D3: comments endpoint no longer uses bare per_page=100 — uses paginated path
    #[test]
    fn fetch_pr_comments_uses_paginated_path_not_flat_per_page() {
        let mut server = mockito::Server::new();

        server.mock("GET", "/repos/owner/repo/pulls/44")
            .with_status(200).with_header("content-type", "application/json")
            .with_body(r#"{"number":44,"title":"t","draft":false,"head":{"ref":"b"},"user":{"login":"author"}}"#)
            .create();
        server.mock("GET", "/repos/owner/repo/commits/b/check-runs")
            .with_status(200).with_header("content-type", "application/json")
            .with_body(r#"{"check_runs":[]}"#).create();
        server.mock("GET", "/repos/owner/repo/branches/b/protection")
            .with_status(404).create();
        server.mock("GET", "/repos/owner/repo/pulls/44/reviews")
            .with_status(200).with_header("content-type", "application/json")
            .with_body(r#"[]"#).create();

        // Only register the paginated path (page=1), NOT the flat per_page=100 path
        server.mock("GET", "/repos/owner/repo/pulls/44/comments?per_page=100&page=1")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[]"#)
            .create();
        server.mock("GET", "/repos/owner/repo/issues/44/comments?per_page=100&page=1")
            .with_status(200).with_header("content-type","application/json").with_body(r#"[]"#).create();

        // If fetch_pr still uses ?per_page=100 (no page=), this will 404 and the test will fail
        let pr = mock_client(&server).fetch_pr("owner", "repo", 44).unwrap();
        assert_eq!(pr.threads.len(), 0);
    }

    // D3-a: check-run deduplication — when same name appears twice, keep only latest started_at
    #[test]
    fn check_run_dedup_keeps_latest_by_started_at() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/repos/owner/repo/pulls/50")
            .with_status(200).with_header("content-type", "application/json")
            .with_body(r#"{"number":50,"title":"t","draft":false,"head":{"ref":"br","sha":"sha50"},"base":{"ref":"main"},"user":{"login":"author"}}"#)
            .create();
        // Two runs with same name: old=failure, new=success
        server.mock("GET", "/repos/owner/repo/commits/br/check-runs")
            .with_status(200).with_header("content-type", "application/json")
            .with_body(r#"{"check_runs":[
                {"name":"rspec","status":"completed","conclusion":"failure","started_at":"2026-05-01T00:00:00Z","details_url":null},
                {"name":"rspec","status":"completed","conclusion":"success","started_at":"2026-05-06T00:00:00Z","details_url":null}
            ]}"#)
            .create();
        server.mock("GET", "/repos/owner/repo/branches/main/protection")
            .with_status(404).create();
        server.mock("GET", "/repos/owner/repo/commits/sha50/statuses")
            .with_status(200).with_header("content-type", "application/json")
            .with_body(r#"[]"#).create();
        server.mock("GET", "/repos/owner/repo/pulls/50/reviews")
            .with_status(200).with_header("content-type", "application/json")
            .with_body(r#"[]"#).create();
        server.mock("GET", "/repos/owner/repo/pulls/50/comments?per_page=100&page=1")
            .with_status(200).with_header("content-type", "application/json")
            .with_body(r#"[]"#).create();
        server.mock("GET", "/repos/owner/repo/issues/50/comments?per_page=100&page=1")
            .with_status(200).with_header("content-type","application/json").with_body(r#"[]"#).create();

        let pr = mock_client(&server).fetch_pr("owner", "repo", 50).unwrap();
        let rspec_checks: Vec<_> = pr.checks.iter().filter(|c| c.name == "rspec").collect();
        assert_eq!(rspec_checks.len(), 1, "expected exactly 1 rspec check after dedup");
        assert_eq!(rspec_checks[0].status, crate::model::CheckStatus::Pass,
            "expected latest (passing) run to win");
    }

    // D3-b: commit status dedup is already handled by seen_contexts; this verifies the second
    // occurrence of the same context is dropped even when it has a different state
    #[test]
    fn commit_status_dedup_drops_duplicate_context() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/repos/owner/repo/pulls/51")
            .with_status(200).with_header("content-type", "application/json")
            .with_body(r#"{"number":51,"title":"t","draft":false,"head":{"ref":"br51","sha":"sha51"},"base":{"ref":"main"},"user":{"login":"author"}}"#)
            .create();
        server.mock("GET", "/repos/owner/repo/commits/br51/check-runs")
            .with_status(200).with_header("content-type", "application/json")
            .with_body(r#"{"check_runs":[]}"#).create();
        server.mock("GET", "/repos/owner/repo/branches/main/protection")
            .with_status(404).create();
        // Two statuses with same context: first=success (most recent per API), second=failure (older)
        server.mock("GET", "/repos/owner/repo/commits/sha51/statuses")
            .with_status(200).with_header("content-type", "application/json")
            .with_body(r#"[
                {"context":"buildkite/primary","state":"success","target_url":"https://buildkite.com/org/p/builds/1"},
                {"context":"buildkite/primary","state":"failure","target_url":"https://buildkite.com/org/p/builds/0"}
            ]"#).create();
        server.mock("GET", "/repos/owner/repo/pulls/51/reviews")
            .with_status(200).with_header("content-type", "application/json")
            .with_body(r#"[]"#).create();
        server.mock("GET", "/repos/owner/repo/pulls/51/comments?per_page=100&page=1")
            .with_status(200).with_header("content-type", "application/json")
            .with_body(r#"[]"#).create();
        server.mock("GET", "/repos/owner/repo/issues/51/comments?per_page=100&page=1")
            .with_status(200).with_header("content-type","application/json").with_body(r#"[]"#).create();

        let pr = mock_client(&server).fetch_pr("owner", "repo", 51).unwrap();
        let bk: Vec<_> = pr.checks.iter().filter(|c| c.name == "buildkite/primary").collect();
        assert_eq!(bk.len(), 1, "expected exactly 1 buildkite/primary status after dedup");
        assert_eq!(bk[0].status, crate::model::CheckStatus::Pass,
            "expected first (most recent) status entry to win");
    }

    // D2-a: fetch_resolved_threads returns threads with resolved state
    #[test]
    fn fetch_resolved_threads_returns_resolved() {
        use crate::github::fetch_resolved_threads;
        // A resolved thread: root comment with resolved_at set (GitHub marks via pull_request_review_threads API)
        // fp models resolved threads as ThreadState::Resolved
        let threads = vec![
            crate::model::Thread {
                id: 1,
                state: crate::model::ThreadState::Resolved,
                author: "".to_string(),
                body: "please fix this".to_string(),
                replies: vec![],
                file: Some("src/main.rs".to_string()),
                line: Some(42),
            },
            crate::model::Thread {
                id: 2,
                state: crate::model::ThreadState::Open,
                author: "".to_string(),
                body: "another issue".to_string(),
                replies: vec![],
                file: None,
                line: None,
            },
        ];
        let resolved = fetch_resolved_threads(&threads);
        assert_eq!(resolved.len(), 1, "expected only 1 resolved thread, got: {}", resolved.len());
        assert_eq!(resolved[0].id, 1);
    }

    // G2: agent_context_manifest returns JSON with required top-level keys
    #[test]
    fn agent_context_manifest_contains_required_keys() {
        use crate::github::agent_context_manifest;
        let m = agent_context_manifest();
        assert!(m.get("name").is_some(), "manifest missing 'name' key");
        assert!(m.get("commands").is_some(), "manifest missing 'commands' key");
        assert!(m.get("auth_required").is_some(), "manifest missing 'auth_required' key");
        assert!(m["commands"].is_array(), "'commands' should be an array");
        assert!(m["commands"].as_array().unwrap().len() > 0, "'commands' array should not be empty");
    }

    // G1: fetch_open_threads returns only Open and Stale threads, excludes Addressed and Resolved
    #[test]
    fn fetch_open_threads_excludes_addressed_and_resolved() {
        use crate::github::fetch_open_threads;
        use crate::model::{Thread, ThreadState};
        let threads = vec![
            Thread { id: 1, state: ThreadState::Open, author: "".into(), body: "open".into(), replies: vec![], file: None, line: None },
            Thread { id: 2, state: ThreadState::Stale, author: "".into(), body: "stale".into(), replies: vec![], file: None, line: None },
            Thread { id: 3, state: ThreadState::Addressed, author: "".into(), body: "addressed".into(), replies: vec![], file: None, line: None },
            Thread { id: 4, state: ThreadState::Resolved, author: "".into(), body: "resolved".into(), replies: vec![], file: None, line: None },
        ];
        let open = fetch_open_threads(&threads);
        assert_eq!(open.len(), 2, "expected 2 open/stale threads, got: {}", open.len());
        assert!(open.iter().any(|t| t.id == 1), "should include Open thread");
        assert!(open.iter().any(|t| t.id == 2), "should include Stale thread");
        assert!(!open.iter().any(|t| t.id == 3), "should exclude Addressed thread");
        assert!(!open.iter().any(|t| t.id == 4), "should exclude Resolved thread");
    }

    // D4-a: resolve_track_branch returns fetched branch when explicit is absent
    #[test]
    fn resolve_track_branch_uses_fetched_when_explicit_absent() {
        use crate::github::resolve_track_branch;
        let result = resolve_track_branch(None, Some("feature/foo".to_string()), 99);
        assert_eq!(result.unwrap(), "feature/foo");
    }

    // D4-b: resolve_track_branch errors with corrective message when both fail
    #[test]
    fn resolve_track_branch_errors_with_corrective_message() {
        use crate::github::resolve_track_branch;
        let err = resolve_track_branch(None, None, 99).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("fp track 99 --branch"), "error should contain corrective command, got: {msg}");
    }

    // D4-c: resolve_track_branch prefers explicit over fetched
    #[test]
    fn resolve_track_branch_prefers_explicit() {
        use crate::github::resolve_track_branch;
        let result = resolve_track_branch(Some("explicit-branch".to_string()), Some("fetched-branch".to_string()), 99);
        assert_eq!(result.unwrap(), "explicit-branch");
    }

    // D1-a: resolve_github_token uses gh_token fallback when GITHUB_TOKEN is absent
    #[test]
    fn resolve_github_token_uses_gh_fallback() {
        use crate::github::resolve_github_token_with;
        let result = resolve_github_token_with(None, Some("gh-token-from-cli".to_string()));
        assert_eq!(result.unwrap(), "gh-token-from-cli");
    }

    // D1-b: resolve_github_token errors with enumerated remediation when both sources fail
    #[test]
    fn resolve_github_token_error_enumerates_options() {
        use crate::github::resolve_github_token_with;
        let err = resolve_github_token_with(None, None).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("export GITHUB_TOKEN"), "error should mention export GITHUB_TOKEN, got: {msg}");
        assert!(msg.contains("gh auth login"), "error should mention gh auth login, got: {msg}");
    }

    // D1-c: resolve_github_token returns env var when set, without calling gh
    #[test]
    fn resolve_github_token_prefers_env_var() {
        use crate::github::resolve_github_token_with;
        let result = resolve_github_token_with(Some("env-token".to_string()), None);
        assert_eq!(result.unwrap(), "env-token");
    }

    // C1: post_pr_comment posts body to issues comments API and returns the posted URL
    #[test]
    fn post_pr_comment_calls_issues_api() {
        let mut server = mockito::Server::new();
        server.mock("POST", "/repos/owner/repo/issues/55/comments")
            .with_status(201)
            .with_header("content-type", "application/json")
            .with_body(r#"{"html_url":"https://github.com/owner/repo/issues/55#issuecomment-999","body":"hello"}"#)
            .create();

        let result = mock_client(&server).post_pr_comment("owner", "repo", 55, "hello").unwrap();
        assert!(result.contains("issuecomment"), "should return the comment URL, got: {result}");
    }

    // RR1: approved is false when requested_reviewers has pending teams, even with an APPROVED review
    #[test]
    fn approved_false_when_requested_reviewers_teams_pending() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/repos/owner/repo/pulls/99")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"{"number":99,"title":"t","draft":false,"head":{"ref":"b","sha":"sha99"},"base":{"ref":"main"},"user":{"login":"author"}}"#)
            .create();
        server.mock("GET", "/repos/owner/repo/commits/b/check-runs")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"{"check_runs":[]}"#).create();
        server.mock("GET", "/repos/owner/repo/branches/main/protection")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"{"required_status_checks":{"contexts":[]}}"#).create();
        server.mock("GET", "/repos/owner/repo/branches/b/protection")
            .with_status(404).create();
        server.mock("GET", "/repos/owner/repo/commits/sha99/statuses")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"[]"#).create();
        // One APPROVED review exists
        server.mock("GET", "/repos/owner/repo/pulls/99/reviews")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"[{"state":"APPROVED","user":{"login":"reviewer1"}}]"#).create();
        // But a team review is still pending
        server.mock("GET", "/repos/owner/repo/pulls/99/requested_reviewers")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"{"users":[],"teams":[{"slug":"codeowners-team"}]}"#).create();
        server.mock("GET", "/repos/owner/repo/pulls/99/comments?per_page=100&page=1")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"[]"#).create();
        server.mock("GET", "/repos/owner/repo/issues/99/comments?per_page=100&page=1")
            .with_status(200).with_header("content-type","application/json").with_body(r#"[]"#).create();

        let pr = mock_client(&server).fetch_pr("owner", "repo", 99).unwrap();
        assert!(!pr.approved, "approved must be false when teams are still pending in requested_reviewers");
    }

    // CR2: create_pr sends body field in POST payload when provided
    #[test]
    fn create_pr_sends_body_to_api() {
        let mut server = mockito::Server::new();
        server.mock("POST", "/repos/owner/repo/pulls")
            .match_body(mockito::Matcher::PartialJson(serde_json::json!({"body": "my description"})))
            .with_status(201)
            .with_header("content-type", "application/json")
            .with_body(r#"{"number":10,"html_url":"https://github.com/owner/repo/pull/10","head":{"ref":"feat/x"},"title":"my PR"}"#)
            .create();

        let result = mock_client(&server).create_pr_with_body("owner", "repo", "my PR", "feat/x", "main", false, Some("my description")).unwrap();
        assert_eq!(result.number, 10);
    }

    // RS3: update_pr_base sends PATCH to update base branch
    #[test]
    fn update_pr_base_calls_patch_endpoint() {
        let mut server = mockito::Server::new();
        server.mock("PATCH", "/repos/owner/repo/pulls/42")
            .match_body(mockito::Matcher::PartialJson(serde_json::json!({"base": "feat/new"})))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"number":42,"head":{"ref":"feat/old"},"base":{"ref":"feat/new"},"title":"old PR"}"#)
            .create();

        mock_client(&server).update_pr_base("owner", "repo", 42, "feat/new").unwrap();
    }

    // RD1: mark_pr_ready fetches node_id then calls GraphQL markPullRequestReadyForReview
    #[test]
    fn mark_pr_ready_sends_graphql_mutation() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/repos/owner/repo/pulls/42")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"number":42,"node_id":"PR_abc123","head":{"ref":"feat/x"},"base":{"ref":"main"},"title":"t","draft":true}"#)
            .create();
        server.mock("POST", "/graphql")
            .match_body(mockito::Matcher::PartialJsonString(
                r#"{"query":"mutation { markPullRequestReadyForReview(input: { pullRequestId: \"PR_abc123\" }) { pullRequest { isDraft } } }"}"#.into()
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data":{"markPullRequestReadyForReview":{"pullRequest":{"isDraft":false}}}}"#)
            .create();

        mock_client(&server).mark_pr_ready("owner", "repo", 42).unwrap();
    }

    // RS4: fetch_pr_base returns the base branch name for a given PR number
    #[test]
    fn fetch_pr_base_returns_base_branch() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/repos/owner/repo/pulls/77")
            .with_status(200).with_header("content-type","application/json")
            .with_body(r#"{"number":77,"head":{"ref":"feat/child"},"base":{"ref":"feat/parent"},"title":"child PR"}"#)
            .create();

        let base = mock_client(&server).fetch_pr_base("owner", "repo", 77).unwrap();
        assert_eq!(base, "feat/parent");
    }

    // MG1: fetch_pr_head_sha_and_base returns head.sha and base.ref for a merged PR
    #[test]
    fn fetch_pr_head_sha_and_base_returns_correct_fields() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/repos/owner/repo/pulls/99")
            .with_status(200).with_header("content-type", "application/json")
            .with_body(r#"{"number":99,"state":"closed","merged":true,"head":{"sha":"abc123def","ref":"feat/merged"},"base":{"ref":"main"},"title":"merged PR"}"#)
            .create();

        let (sha, base) = mock_client(&server).fetch_pr_head_sha_and_base("owner", "repo", 99).unwrap();
        assert_eq!(sha, "abc123def");
        assert_eq!(base, "main");
    }

    // MG2: fetch_pr_is_merged returns true for merged PR, false for open PR
    #[test]
    fn fetch_pr_is_merged_returns_correct_state() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/repos/owner/repo/pulls/10")
            .with_status(200).with_header("content-type", "application/json")
            .with_body(r#"{"number":10,"state":"open","merged":false,"head":{"sha":"aaa"},"base":{"ref":"main"},"title":"open PR"}"#)
            .create();
        server.mock("GET", "/repos/owner/repo/pulls/11")
            .with_status(200).with_header("content-type", "application/json")
            .with_body(r#"{"number":11,"state":"closed","merged":true,"head":{"sha":"bbb"},"base":{"ref":"main"},"title":"merged PR"}"#)
            .create();

        assert!(!mock_client(&server).fetch_pr_is_merged("owner", "repo", 10).unwrap());
        assert!(mock_client(&server).fetch_pr_is_merged("owner", "repo", 11).unwrap());
    }

    fn minimal_pr_mocks(server: &mut mockito::Server, pr_number: u64, branch: &str) {
        let pr_body = format!(
            r#"{{"number":{pr},"title":"t","draft":false,"head":{{"ref":"{br}","sha":""}},"base":{{"ref":"main"}}}}"#,
            pr = pr_number, br = branch
        );
        let encoded_branch = branch.replace('/', "%2F");
        server.mock("GET", format!("/repos/owner/repo/pulls/{}", pr_number).as_str())
            .with_status(200).with_header("content-type","application/json").with_body(pr_body).create();
        server.mock("GET", format!("/repos/owner/repo/commits/{}/check-runs", encoded_branch).as_str())
            .with_status(200).with_header("content-type","application/json").with_body(r#"{"check_runs":[]}"#).create();
        server.mock("GET", format!("/repos/owner/repo/branches/{}/protection", encoded_branch).as_str())
            .with_status(404).create();
        // Shared mocks set to expect multiple hits
        server.mock("GET", format!("/repos/owner/repo/branches/main/protection").as_str())
            .with_status(404).expect_at_least(1).create();
        server.mock("GET", format!("/repos/owner/repo/commits//statuses").as_str())
            .with_status(200).with_header("content-type","application/json").with_body(r#"[]"#)
            .expect_at_least(1).create();
        server.mock("GET", format!("/repos/owner/repo/pulls/{}/reviews", pr_number).as_str())
            .with_status(200).with_header("content-type","application/json").with_body(r#"[]"#).create();
        server.mock("GET", format!("/repos/owner/repo/pulls/{}/comments?per_page=100&page=1", pr_number).as_str())
            .with_status(200).with_header("content-type","application/json").with_body(r#"[]"#).create();
        server.mock("GET", format!("/repos/owner/repo/pulls/{}/requested_reviewers", pr_number).as_str())
            .with_status(200).with_header("content-type","application/json").with_body(r#"{"users":[],"teams":[]}"#).create();
        server.mock("GET", format!("/repos/owner/repo/issues/{}/comments?per_page=100&page=1", pr_number).as_str())
            .with_status(200).with_header("content-type","application/json").with_body(r#"[]"#).expect_at_least(1).create();
    }

    // PAR2: fetch_prs_as_map returns a HashMap keyed by PR number
    #[test]
    fn fetch_prs_as_map_returns_map_keyed_by_number() {
        let mut server = mockito::Server::new();
        minimal_pr_mocks(&mut server, 201, "feat/x");
        minimal_pr_mocks(&mut server, 202, "feat/y");

        let client = mock_client(&server);
        let map = client.fetch_prs_as_map("owner", "repo", &[201, 202]);
        assert!(map.contains_key(&201), "expected key 201");
        assert!(map.contains_key(&202), "expected key 202");
        assert_eq!(map[&201].number, 201);
        assert_eq!(map[&202].number, 202);
    }

    // PAR1: fetch_prs_parallel returns results for all requested PR numbers
    #[test]
    fn fetch_prs_parallel_returns_all_results() {
        let mut server = mockito::Server::new();
        minimal_pr_mocks(&mut server, 101, "feat/a");
        minimal_pr_mocks(&mut server, 102, "feat/b");
        minimal_pr_mocks(&mut server, 103, "feat/c");

        let client = mock_client(&server);
        let results = client.fetch_prs_parallel("owner", "repo", &[101, 102, 103]);
        assert_eq!(results.len(), 3, "expected 3 results");
        let numbers: Vec<u64> = {
            let mut v: Vec<u64> = results.iter().map(|r| r.number).collect();
            v.sort();
            v
        };
        assert_eq!(numbers, vec![101, 102, 103]);
    }

    fn full_pr_mocks(server: &mut mockito::Server, pr_number: u64, author: &str, sha: &str, issue_comments_body: &str) {
        let pr_body = format!(
            r#"{{"number":{pr},"title":"t","draft":false,"head":{{"ref":"b","sha":"{sha}"}},"base":{{"ref":"main"}},"user":{{"login":"{author}"}}}}"#,
            pr = pr_number, sha = sha, author = author
        );
        server.mock("GET", format!("/repos/owner/repo/pulls/{}", pr_number).as_str())
            .with_status(200).with_header("content-type","application/json").with_body(pr_body).create();
        server.mock("GET", "/repos/owner/repo/commits/b/check-runs")
            .with_status(200).with_header("content-type","application/json").with_body(r#"{"check_runs":[]}"#).create();
        server.mock("GET", "/repos/owner/repo/branches/main/protection")
            .with_status(404).create();
        server.mock("GET", format!("/repos/owner/repo/commits/{}/statuses", sha).as_str())
            .with_status(200).with_header("content-type","application/json").with_body(r#"[]"#).create();
        server.mock("GET", format!("/repos/owner/repo/pulls/{}/reviews", pr_number).as_str())
            .with_status(200).with_header("content-type","application/json").with_body(r#"[]"#).create();
        server.mock("GET", format!("/repos/owner/repo/pulls/{}/requested_reviewers", pr_number).as_str())
            .with_status(200).with_header("content-type","application/json").with_body(r#"{"users":[],"teams":[]}"#).create();
        server.mock("GET", format!("/repos/owner/repo/pulls/{}/comments?per_page=100&page=1", pr_number).as_str())
            .with_status(200).with_header("content-type","application/json").with_body(r#"[]"#).create();
        server.mock("GET", format!("/repos/owner/repo/issues/{}/comments?per_page=100&page=1", pr_number).as_str())
            .with_status(200).with_header("content-type","application/json").with_body(issue_comments_body).create();
    }

    // IC1: issue comment from non-author with no reply is surfaced as Open thread
    #[test]
    fn issue_comment_from_reviewer_is_open_thread() {
        let mut server = mockito::Server::new();
        let comments = r#"[{"id":1,"body":"please fix this","user":{"login":"reviewer","type":"User"}}]"#;
        full_pr_mocks(&mut server, 77, "author", "sha1", comments);
        let pr = mock_client(&server).fetch_pr("owner", "repo", 77).unwrap();
        let issue_threads: Vec<_> = pr.threads.iter().filter(|t| t.file.is_none()).collect();
        assert_eq!(issue_threads.len(), 1, "expected 1 issue-level thread, got {}", issue_threads.len());
        assert_eq!(issue_threads[0].state, crate::model::ThreadState::Open, "expected Open state");
    }

    // IC2: issue comment from bot is excluded
    #[test]
    fn issue_comment_from_bot_is_excluded() {
        let mut server = mockito::Server::new();
        let comments = r#"[{"id":2,"body":"CI passed","user":{"login":"github-actions[bot]","type":"Bot"}}]"#;
        full_pr_mocks(&mut server, 78, "author", "sha2", comments);
        let pr = mock_client(&server).fetch_pr("owner", "repo", 78).unwrap();
        let issue_threads: Vec<_> = pr.threads.iter().filter(|t| t.file.is_none()).collect();
        assert_eq!(issue_threads.len(), 0, "expected bot comment to be excluded, got {}", issue_threads.len());
    }

    // IC3: issue comment with author reply is Addressed
    #[test]
    fn issue_comment_with_author_reply_is_addressed() {
        let mut server = mockito::Server::new();
        // Two comments: reviewer asks, author replies
        let comments = r#"[
            {"id":3,"body":"please clarify","user":{"login":"reviewer","type":"User"}},
            {"id":4,"body":"done","user":{"login":"author","type":"User"}}
        ]"#;
        full_pr_mocks(&mut server, 79, "author", "sha3", comments);
        let pr = mock_client(&server).fetch_pr("owner", "repo", 79).unwrap();
        let issue_threads: Vec<_> = pr.threads.iter().filter(|t| t.file.is_none()).collect();
        assert_eq!(issue_threads.len(), 1, "expected 1 issue-level thread");
        assert_eq!(issue_threads[0].state, crate::model::ThreadState::Addressed, "expected Addressed state");
    }

    // IC4: issue comment thread has file=None and line=None
    #[test]
    fn issue_comment_thread_has_no_file_or_line() {
        let mut server = mockito::Server::new();
        let comments = r#"[{"id":5,"body":"general comment","user":{"login":"reviewer","type":"User"}}]"#;
        full_pr_mocks(&mut server, 80, "author", "sha4", comments);
        let pr = mock_client(&server).fetch_pr("owner", "repo", 80).unwrap();
        let issue_threads: Vec<_> = pr.threads.iter().filter(|t| t.file.is_none()).collect();
        assert_eq!(issue_threads.len(), 1, "expected 1 issue-level thread");
        assert!(issue_threads[0].file.is_none(), "expected file to be None");
        assert!(issue_threads[0].line.is_none(), "expected line to be None");
    }

    fn full_pr_mocks_with_reviews(server: &mut mockito::Server, pr_number: u64, author: &str, sha: &str, reviews_body: &str) {
        let pr_body = format!(
            r#"{{"number":{pr},"title":"t","draft":false,"head":{{"ref":"b","sha":"{sha}"}},"base":{{"ref":"main"}},"user":{{"login":"{author}"}}}}"#,
            pr = pr_number, sha = sha, author = author
        );
        server.mock("GET", format!("/repos/owner/repo/pulls/{}", pr_number).as_str())
            .with_status(200).with_header("content-type","application/json").with_body(pr_body).create();
        server.mock("GET", "/repos/owner/repo/commits/b/check-runs")
            .with_status(200).with_header("content-type","application/json").with_body(r#"{"check_runs":[]}"#).create();
        server.mock("GET", "/repos/owner/repo/branches/main/protection")
            .with_status(404).create();
        server.mock("GET", format!("/repos/owner/repo/commits/{}/statuses", sha).as_str())
            .with_status(200).with_header("content-type","application/json").with_body(r#"[]"#).create();
        server.mock("GET", format!("/repos/owner/repo/pulls/{}/reviews", pr_number).as_str())
            .with_status(200).with_header("content-type","application/json").with_body(reviews_body).create();
        server.mock("GET", format!("/repos/owner/repo/pulls/{}/requested_reviewers", pr_number).as_str())
            .with_status(200).with_header("content-type","application/json").with_body(r#"{"users":[],"teams":[]}"#).create();
        server.mock("GET", format!("/repos/owner/repo/pulls/{}/comments?per_page=100&page=1", pr_number).as_str())
            .with_status(200).with_header("content-type","application/json").with_body(r#"[]"#).create();
        server.mock("GET", format!("/repos/owner/repo/issues/{}/comments?per_page=100&page=1", pr_number).as_str())
            .with_status(200).with_header("content-type","application/json").with_body(r#"[]"#).create();
    }

    // RV1: review with CHANGES_REQUESTED and body surfaces as Open thread
    #[test]
    fn review_changes_requested_with_body_is_open_thread() {
        let mut server = mockito::Server::new();
        let reviews = r#"[{"id":100,"state":"CHANGES_REQUESTED","body":"Please fix the naming","user":{"login":"reviewer","type":"User"},"submitted_at":"2024-01-01T00:00:00Z"}]"#;
        full_pr_mocks_with_reviews(&mut server, 90, "author", "sha90", reviews);
        let pr = mock_client(&server).fetch_pr("owner", "repo", 90).unwrap();
        let review_threads: Vec<_> = pr.threads.iter().filter(|t| t.file.is_none()).collect();
        assert_eq!(review_threads.len(), 1, "expected 1 review thread, got {}", review_threads.len());
        assert_eq!(review_threads[0].state, crate::model::ThreadState::Open, "expected Open");
    }

    // RV2: review with APPROVED state is excluded even with body
    #[test]
    fn review_approved_is_excluded() {
        let mut server = mockito::Server::new();
        let reviews = r#"[{"id":101,"state":"APPROVED","body":"LGTM","user":{"login":"reviewer","type":"User"},"submitted_at":"2024-01-01T00:00:00Z"}]"#;
        full_pr_mocks_with_reviews(&mut server, 91, "author", "sha91", reviews);
        let pr = mock_client(&server).fetch_pr("owner", "repo", 91).unwrap();
        let review_threads: Vec<_> = pr.threads.iter().filter(|t| t.file.is_none()).collect();
        assert_eq!(review_threads.len(), 0, "expected APPROVED review to be excluded, got {}", review_threads.len());
    }

    // RV3: review with empty body is excluded
    #[test]
    fn review_with_empty_body_is_excluded() {
        let mut server = mockito::Server::new();
        let reviews = r#"[{"id":102,"state":"COMMENTED","body":"","user":{"login":"reviewer","type":"User"},"submitted_at":"2024-01-01T00:00:00Z"}]"#;
        full_pr_mocks_with_reviews(&mut server, 92, "author", "sha92", reviews);
        let pr = mock_client(&server).fetch_pr("owner", "repo", 92).unwrap();
        let review_threads: Vec<_> = pr.threads.iter().filter(|t| t.file.is_none()).collect();
        assert_eq!(review_threads.len(), 0, "expected empty-body review to be excluded, got {}", review_threads.len());
    }

    // RV4: review thread has file=None and line=None
    #[test]
    fn review_thread_has_no_file_or_line() {
        let mut server = mockito::Server::new();
        let reviews = r#"[{"id":103,"state":"CHANGES_REQUESTED","body":"Fix this","user":{"login":"reviewer","type":"User"},"submitted_at":"2024-01-01T00:00:00Z"}]"#;
        full_pr_mocks_with_reviews(&mut server, 93, "author", "sha93", reviews);
        let pr = mock_client(&server).fetch_pr("owner", "repo", 93).unwrap();
        let review_threads: Vec<_> = pr.threads.iter().filter(|t| t.file.is_none()).collect();
        assert_eq!(review_threads.len(), 1, "expected 1 review thread");
        assert!(review_threads[0].file.is_none(), "expected file to be None");
        assert!(review_threads[0].line.is_none(), "expected line to be None");
    }

    // RV5: review from author is excluded
    #[test]
    fn review_from_author_is_excluded() {
        let mut server = mockito::Server::new();
        let reviews = r#"[{"id":104,"state":"COMMENTED","body":"I updated this","user":{"login":"author","type":"User"},"submitted_at":"2024-01-01T00:00:00Z"}]"#;
        full_pr_mocks_with_reviews(&mut server, 94, "author", "sha94", reviews);
        let pr = mock_client(&server).fetch_pr("owner", "repo", 94).unwrap();
        let review_threads: Vec<_> = pr.threads.iter().filter(|t| t.file.is_none()).collect();
        assert_eq!(review_threads.len(), 0, "expected author review to be excluded, got {}", review_threads.len());
    }

    // IC5: issue comment from PR author is excluded (only show comments needing a response)
    #[test]
    fn issue_comment_from_author_is_excluded() {
        let mut server = mockito::Server::new();
        let comments = r#"[{"id":6,"body":"I updated the PR","user":{"login":"author","type":"User"}}]"#;
        full_pr_mocks(&mut server, 81, "author", "sha5", comments);
        let pr = mock_client(&server).fetch_pr("owner", "repo", 81).unwrap();
        let issue_threads: Vec<_> = pr.threads.iter().filter(|t| t.file.is_none()).collect();
        assert_eq!(issue_threads.len(), 0, "expected author's own comment to be excluded, got {}", issue_threads.len());
    }
}
