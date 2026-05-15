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
                "head": { "ref": "fix/foo" },
                "user": { "login": "author" }
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
            .mock("GET", "/repos/owner/repo/pulls/42/reviews?per_page=100&page=1")
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
            .with_body(r#"{"number":1,"title":"draft","draft":true,"head":{"ref":"wip"},"user":{"login":"author"}}"#)
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
            .mock("GET", "/repos/owner/repo/pulls/1/reviews?per_page=100&page=1")
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
            .with_body(r#"{"number":5,"title":"t","draft":false,"head":{"ref":"mybranch","sha":""},"base":{"ref":"main"},"user":{"login":"author"}}"#)
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
            .mock("GET", "/repos/owner/repo/pulls/5/reviews?per_page=100&page=1")
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
            .with_body(r#"{"number":55,"title":"t","draft":false,"head":{"ref":"br","sha":""},"base":{"ref":"main"},"user":{"login":"author"}}"#)
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
        server.mock("GET", "/repos/owner/repo/pulls/55/reviews?per_page=100&page=1")
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
            .with_body(r#"{"number":6,"title":"t","draft":false,"head":{"ref":"b"},"user":{"login":"author"}}"#)
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
            .mock("GET", "/repos/owner/repo/pulls/6/reviews?per_page=100&page=1")
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
            .mock("GET", "/repos/owner/repo/pulls/7/reviews?per_page=100&page=1")
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
            .mock("GET", "/repos/owner/repo/pulls/11/reviews?per_page=100&page=1")
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
        server.mock("GET", "/repos/owner/repo/pulls/88/reviews?per_page=100&page=1")
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
        server.mock("GET", "/repos/owner/repo/pulls/77/reviews?per_page=100&page=1")
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
            .mock("GET", "/repos/owner/repo/pulls/12/reviews?per_page=100&page=1")
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
            .with_body(r#"{"number":8,"title":"t","draft":false,"head":{"ref":"b"},"user":{"login":"author"}}"#)
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
            .mock("GET", "/repos/owner/repo/pulls/8/reviews?per_page=100&page=1")
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
            .with_body(r#"{"number":9,"title":"t","draft":false,"head":{"ref":"b"},"user":{"login":"author"}}"#)
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
            .mock("GET", "/repos/owner/repo/pulls/9/reviews?per_page=100&page=1")
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
            .with_body(r#"{"number":10,"title":"t","draft":false,"head":{"ref":"b"},"user":{"login":"author"}}"#)
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
            .mock("GET", "/repos/owner/repo/pulls/10/reviews?per_page=100&page=1")
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
        server.mock("GET", "/repos/owner/repo/pulls/95/reviews?per_page=100&page=1").with_status(200).with_header("content-type","application/json").with_body(reviews).create();
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
        server.mock("GET", "/repos/owner/repo/pulls/96/reviews?per_page=100&page=1").with_status(200).with_header("content-type","application/json").with_body(reviews).create();
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
        server.mock("GET", "/repos/owner/repo/pulls/97/reviews?per_page=100&page=1").with_status(200).with_header("content-type","application/json").with_body(reviews).create();
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
                r#"{"number":42,"title":"my feature","draft":false,"head":{"ref":"feat/thing"},"user":{"login":"author"}}"#,
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
            .mock("GET", "/repos/owner/repo/pulls/30/reviews?per_page=100&page=1")
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
            .mock("GET", "/repos/owner/repo/pulls/20/reviews?per_page=100&page=1")
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
        server.mock("GET", "/repos/owner/repo/pulls/55/reviews?per_page=100&page=1")
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
            .mock("GET", "/repos/owner/repo/pulls/42/reviews?per_page=100&page=1")
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
        server.mock("GET", format!("/repos/owner/repo/pulls/{}/reviews?per_page=100&page=1", pr_number).as_str())
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
        server.mock("GET", "/repos/owner/repo/pulls/43/reviews?per_page=100&page=1")
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

    // D_reviews_pagination: reviews endpoint uses get_paginated so reviews beyond page 1 are visible
    #[test]
    fn fetch_pr_paginates_reviews_across_multiple_pages() {
        let mut server = mockito::Server::new();
        let page2_url = format!("{}/repos/owner/repo/pulls/45/reviews?per_page=100&page=2", server.url());
        let link_header = format!(r#"<{}>; rel="next""#, page2_url);

        server.mock("GET", "/repos/owner/repo/pulls/45")
            .with_status(200).with_header("content-type", "application/json")
            .with_body(r#"{"number":45,"title":"t","draft":false,"head":{"ref":"b"},"user":{"login":"author"}}"#)
            .create();
        server.mock("GET", "/repos/owner/repo/commits/b/check-runs")
            .with_status(200).with_header("content-type", "application/json")
            .with_body(r#"{"check_runs":[]}"#).create();
        server.mock("GET", "/repos/owner/repo/branches/b/protection")
            .with_status(404).create();

        // Page 1: no approvals, Link: next header pointing to page 2
        server.mock("GET", "/repos/owner/repo/pulls/45/reviews?per_page=100&page=1")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_header("link", &link_header)
            .with_body(r#"[]"#)
            .create();
        // Page 2: APPROVED review
        server.mock("GET", "/repos/owner/repo/pulls/45/reviews?per_page=100&page=2")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[{"id":999,"state":"APPROVED","body":"","user":{"login":"reviewer","type":"User"},"submitted_at":"2024-01-02T00:00:00Z"}]"#)
            .create();

        server.mock("GET", "/repos/owner/repo/pulls/45/comments?per_page=100&page=1")
            .with_status(200).with_header("content-type", "application/json").with_body(r#"[]"#).create();
        server.mock("GET", "/repos/owner/repo/issues/45/comments?per_page=100&page=1")
            .with_status(200).with_header("content-type", "application/json").with_body(r#"[]"#).create();
        server.mock("GET", "/repos/owner/repo/pulls/45/requested_reviewers")
            .with_status(200).with_header("content-type", "application/json")
            .with_body(r#"{"users":[],"teams":[]}"#).create();

        let pr = mock_client(&server).fetch_pr("owner", "repo", 45).unwrap();
        assert!(pr.approved, "expected approved=true from page-2 review, got false");
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
        server.mock("GET", "/repos/owner/repo/pulls/44/reviews?per_page=100&page=1")
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
        server.mock("GET", "/repos/owner/repo/pulls/50/reviews?per_page=100&page=1")
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
        server.mock("GET", "/repos/owner/repo/pulls/51/reviews?per_page=100&page=1")
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


    // RT-D1: parse_resolved_review_threads_from_graphql returns resolved_by
    #[test]
    fn parse_resolved_threads_returns_resolver() {
        use crate::github::parse_resolved_review_threads_from_graphql;
        let json = r#"{
            "data": {
                "repository": {
                    "pullRequest": {
                        "reviewThreads": {
                            "nodes": [
                                {
                                    "id": "PRRT_abc",
                                    "isResolved": true,
                                    "resolvedBy": { "login": "alice" },
                                    "comments": {
                                        "nodes": [
                                            { "createdAt": "2024-01-01T10:00:00Z", "body": "Fix this", "path": "src/main.rs", "line": 42 }
                                        ]
                                    }
                                }
                            ]
                        },
                        "commits": { "nodes": [] }
                    }
                }
            }
        }"#;
        let threads = parse_resolved_review_threads_from_graphql(json).unwrap();
        assert_eq!(threads.len(), 1, "expected 1 resolved thread");
        assert_eq!(threads[0].resolved_by.as_deref(), Some("alice"), "expected resolver alice");
    }

    // RT-D2: parse_resolved_review_threads_from_graphql returns resolved_at timestamp
    #[test]
    fn parse_resolved_threads_returns_resolved_at() {
        use crate::github::parse_resolved_review_threads_from_graphql;
        let json = r#"{
            "data": {
                "repository": {
                    "pullRequest": {
                        "reviewThreads": {
                            "nodes": [
                                {
                                    "id": "PRRT_abc",
                                    "isResolved": true,
                                    "resolvedBy": { "login": "alice" },
                                    "comments": {
                                        "nodes": [
                                            { "createdAt": "2024-01-01T10:00:00Z", "body": "Fix this", "path": null, "line": null }
                                        ]
                                    }
                                }
                            ]
                        },
                        "commits": { "nodes": [] }
                    }
                }
            }
        }"#;
        let threads = parse_resolved_review_threads_from_graphql(json).unwrap();
        assert_eq!(threads[0].created_at.as_deref(), Some("2024-01-01T10:00:00Z"), "expected created_at timestamp");
    }

    // RT-D3: parse_resolved_review_threads_from_graphql returns first commit after thread opened
    #[test]
    fn parse_resolved_threads_returns_first_commit_after_opened() {
        use crate::github::parse_resolved_review_threads_from_graphql;
        let json = r#"{
            "data": {
                "repository": {
                    "pullRequest": {
                        "reviewThreads": {
                            "nodes": [
                                {
                                    "id": "PRRT_abc",
                                    "isResolved": true,
                                    "resolvedBy": { "login": "alice" },
                                    "comments": {
                                        "nodes": [
                                            { "createdAt": "2024-01-02T10:00:00Z", "body": "Fix this", "path": null, "line": null }
                                        ]
                                    }
                                }
                            ]
                        },
                        "commits": {
                            "nodes": [
                                { "commit": { "abbreviatedOid": "aaa1111", "committedDate": "2024-01-01T09:00:00Z", "messageHeadline": "before thread" } },
                                { "commit": { "abbreviatedOid": "bbb2222", "committedDate": "2024-01-03T11:00:00Z", "messageHeadline": "after thread" } }
                            ]
                        }
                    }
                }
            }
        }"#;
        let threads = parse_resolved_review_threads_from_graphql(json).unwrap();
        assert_eq!(
            threads[0].first_commit_after_opened.as_deref(),
            Some("bbb2222 after thread"),
            "expected commit pushed after thread opened"
        );
    }

    // RT-D4: parse_resolved_review_threads_from_graphql excludes non-resolved threads
    #[test]
    fn parse_resolved_threads_excludes_open_threads() {
        use crate::github::parse_resolved_review_threads_from_graphql;
        let json = r#"{
            "data": {
                "repository": {
                    "pullRequest": {
                        "reviewThreads": {
                            "nodes": [
                                {
                                    "id": "PRRT_open",
                                    "isResolved": false,
                                    "resolvedBy": null,
                                    "comments": {
                                        "nodes": [
                                            { "createdAt": "2024-01-01T10:00:00Z", "body": "Still open", "path": null, "line": null }
                                        ]
                                    }
                                },
                                {
                                    "id": "PRRT_resolved",
                                    "isResolved": true,
                                    "resolvedBy": { "login": "bob" },
                                    "comments": {
                                        "nodes": [
                                            { "createdAt": "2024-01-01T10:00:00Z", "body": "Resolved", "path": null, "line": null }
                                        ]
                                    }
                                }
                            ]
                        },
                        "commits": { "nodes": [] }
                    }
                }
            }
        }"#;
        let threads = parse_resolved_review_threads_from_graphql(json).unwrap();
        assert_eq!(threads.len(), 1, "expected only 1 resolved thread, got {}", threads.len());
        assert_eq!(threads[0].resolved_by.as_deref(), Some("bob"));
    }

    // FMT-1: format_open_threads returns "no threads" message when empty
    #[test]
    fn format_open_threads_empty_returns_no_threads_message() {
        use crate::github::format_open_threads;
        let out = format_open_threads(5, &[], false);
        assert!(out.contains("No open threads on PR #5"), "expected no-threads message, got: {}", out);
    }

    // FMT-2: format_open_threads returns thread body and id for non-empty
    #[test]
    fn format_open_threads_shows_thread_body() {
        use crate::github::{format_open_threads, fetch_open_threads};
        use crate::model::{Thread, ThreadState};
        let threads_data = vec![
            Thread { id: 42, state: ThreadState::Open, author: "alice".into(), body: "needs a test".into(), replies: vec![], file: Some("src/lib.rs".into()), line: Some(10) },
        ];
        let open: Vec<&Thread> = fetch_open_threads(&threads_data);
        let out = format_open_threads(7, &open, false);
        assert!(out.contains("needs a test"), "expected thread body in output, got: {}", out);
        assert!(out.contains("#42"), "expected thread id in output, got: {}", out);
    }

    // FMT-3: format_open_threads returns JSON when json=true
    #[test]
    fn format_open_threads_json_mode() {
        use crate::github::{format_open_threads, fetch_open_threads};
        use crate::model::{Thread, ThreadState};
        let threads_data = vec![
            Thread { id: 99, state: ThreadState::Open, author: "bob".into(), body: "json body".into(), replies: vec![], file: None, line: None },
        ];
        let open: Vec<&Thread> = fetch_open_threads(&threads_data);
        let out = format_open_threads(3, &open, true);
        let parsed: serde_json::Value = serde_json::from_str(&out).expect("json output must be valid JSON");
        assert!(parsed.is_array(), "expected JSON array");
    }

    // FMT-4: format_resolved_threads returns "no resolved threads" when empty
    #[test]
    fn format_resolved_threads_empty_returns_no_threads_message() {
        use crate::github::format_resolved_threads;
        let out = format_resolved_threads(8, &[], false);
        assert!(out.contains("No resolved threads on PR #8"), "expected no-resolved-threads message, got: {}", out);
    }

    // FMT-5: format_resolved_threads shows resolver identity
    #[test]
    fn format_resolved_threads_shows_resolver() {
        use crate::github::{format_resolved_threads, ResolvedThreadInfo};
        let threads = vec![ResolvedThreadInfo {
            body: "fix the thing".into(), file: None, line: None,
            resolved_by: Some("dave".into()), created_at: Some("2024-03-01T00:00:00Z".into()),
            first_commit_after_opened: Some("abc1234 add fix".into()),
        }];
        let out = format_resolved_threads(11, &threads, false);
        assert!(out.contains("dave"), "expected resolver name in output, got: {}", out);
        assert!(out.contains("fix the thing"), "expected thread body in output, got: {}", out);
    }

    // RT-D5: fetch_resolved_threads_graphql calls GraphQL endpoint and returns resolved threads
    #[test]
    fn fetch_resolved_threads_graphql_returns_resolved_threads() {
        use crate::github::GithubClient;
        let mut server = mockito::Server::new();
        let graphql_response = serde_json::json!({
            "data": {
                "repository": {
                    "pullRequest": {
                        "reviewThreads": {
                            "nodes": [
                                {
                                    "id": "PRRT_xyz",
                                    "isResolved": true,
                                    "resolvedBy": { "login": "carol" },
                                    "comments": {
                                        "nodes": [
                                            { "createdAt": "2024-02-01T12:00:00Z", "body": "Please add a test", "path": "src/lib.rs", "line": 10 }
                                        ]
                                    }
                                }
                            ]
                        },
                        "commits": { "nodes": [] }
                    }
                }
            }
        });
        server.mock("POST", "/graphql")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(graphql_response.to_string())
            .create();
        let client = GithubClient::with_base_url("token".into(), server.url());
        let threads = client.fetch_resolved_threads_graphql("owner", "repo", 1).unwrap();
        assert_eq!(threads.len(), 1, "expected 1 resolved thread from graphql");
        assert_eq!(threads[0].resolved_by.as_deref(), Some("carol"));
        assert_eq!(threads[0].created_at.as_deref(), Some("2024-02-01T12:00:00Z"));
        assert_eq!(threads[0].file.as_deref(), Some("src/lib.rs"));
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
        server.mock("GET", "/repos/owner/repo/pulls/99/reviews?per_page=100&page=1")
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

    // MG3: merge_pr calls PUT endpoint and returns merge commit sha
    #[test]
    fn merge_pr_calls_put_endpoint_and_returns_sha() {
        let mut server = mockito::Server::new();
        server.mock("PUT", "/repos/owner/repo/pulls/42/merge")
            .with_status(200).with_header("content-type", "application/json")
            .with_body(r#"{"sha":"mergesha123","merged":true,"message":"Pull Request successfully merged"}"#)
            .create();

        let sha = mock_client(&server).merge_pr("owner", "repo", 42, None).unwrap();
        assert_eq!(sha, "mergesha123");
    }

    // MG4: merge_pr passes merge_method when specified
    #[test]
    fn merge_pr_passes_merge_method_when_specified() {
        let mut server = mockito::Server::new();
        server.mock("PUT", "/repos/owner/repo/pulls/43/merge")
            .match_body(mockito::Matcher::PartialJsonString(r#"{"merge_method":"squash"}"#.to_string()))
            .with_status(200).with_header("content-type", "application/json")
            .with_body(r#"{"sha":"squashsha456","merged":true,"message":"Pull Request successfully merged"}"#)
            .create();

        let sha = mock_client(&server).merge_pr("owner", "repo", 43, Some("squash")).unwrap();
        assert_eq!(sha, "squashsha456");
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
            r#"{{"number":{pr},"title":"t","draft":false,"head":{{"ref":"{br}","sha":""}},"base":{{"ref":"main"}},"user":{{"login":"author"}}}}"#,
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
        server.mock("GET", format!("/repos/owner/repo/pulls/{}/reviews?per_page=100&page=1", pr_number).as_str())
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
        server.mock("GET", format!("/repos/owner/repo/pulls/{}/reviews?per_page=100&page=1", pr_number).as_str())
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
        server.mock("GET", format!("/repos/owner/repo/pulls/{}/reviews?per_page=100&page=1", pr_number).as_str())
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

    // CTX1: fetch_checks_for_sha returns checks with correct status and details_url
    #[test]
    fn fetch_checks_for_sha_returns_checks_with_status() {
        use crate::model::CheckStatus;
        let mut server = mockito::Server::new();
        let check_runs = r#"{
            "check_runs": [
                {"name": "build", "conclusion": "failure", "details_url": "https://ci.example.com/runs/1", "status": "completed"},
                {"name": "lint", "conclusion": "success", "details_url": "https://ci.example.com/runs/2", "status": "completed"},
                {"name": "test", "conclusion": "failure", "details_url": "https://ci.example.com/runs/3", "status": "completed"}
            ]
        }"#;
        server.mock("GET", "/repos/owner/repo/commits/abc123/check-runs?per_page=100&page=1")
            .with_status(200).with_header("content-type", "application/json")
            .with_body(check_runs).create();

        let client = mock_client(&server);
        let checks = client.fetch_checks_for_sha("owner", "repo", "abc123").unwrap();
        assert_eq!(checks.len(), 3, "expected 3 checks total");
        let failed: Vec<_> = checks.iter().filter(|c| c.status == CheckStatus::Fail).collect();
        assert_eq!(failed.len(), 2, "expected 2 failed checks");
        assert!(failed.iter().any(|c| c.name == "build"), "expected 'build' in failures");
        assert!(failed.iter().any(|c| c.name == "test"), "expected 'test' in failures");
        assert!(failed[0].details_url.is_some(), "expected details_url on failed check");
    }

    // ADR-007: parse_gh_image_output extracts URL from gh image markdown output
    #[test]
    fn parse_gh_image_output_extracts_url() {
        let output = "![screenshot.gif](https://github.com/user-attachments/assets/abc-123)\n";
        let url = crate::github::parse_gh_image_output(output).unwrap();
        assert_eq!(url, "https://github.com/user-attachments/assets/abc-123");
    }

    // ADR-007: parse_gh_image_output returns error on unexpected format
    #[test]
    fn parse_gh_image_output_errors_on_invalid() {
        let output = "some unexpected output";
        assert!(crate::github::parse_gh_image_output(output).is_err(),
            "expected error on non-markdown output");
    }

    // ADR-007 bug fix: fetch_pr_body returns current PR body string
    #[test]
    fn fetch_pr_body_returns_current_body() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/repos/owner/repo/pulls/42")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"number": 42, "body": "existing PR description"}"#)
            .create();
        let client = mock_client(&server);
        let body = client.fetch_pr_body("owner", "repo", 42).unwrap();
        assert_eq!(body, "existing PR description", "expected current PR body");
    }

    // ADR-007 bug fix: fetch_pr_body returns empty string when body is null
    #[test]
    fn fetch_pr_body_returns_empty_when_null() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/repos/owner/repo/pulls/42")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"number": 42, "body": null}"#)
            .create();
        let client = mock_client(&server);
        let body = client.fetch_pr_body("owner", "repo", 42).unwrap();
        assert_eq!(body, "", "null body should return empty string");
    }

    // ADR-007: update_pr sends PATCH with title and/or body fields
    #[test]
    fn update_pr_sends_patch_with_title_and_body() {
        let mut server = mockito::Server::new();
        let _m = server.mock("PATCH", "/repos/owner/repo/pulls/42")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"number": 42}"#)
            .match_body(mockito::Matcher::PartialJsonString(
                r#"{"title":"new title","body":"new body"}"#.to_string()
            ))
            .create();
        let client = mock_client(&server);
        client.update_pr("owner", "repo", 42, Some("new title"), Some("new body")).unwrap();
        _m.assert();
    }

    // ADR-007: update_pr with only body (no title) sends just body field
    #[test]
    fn update_pr_sends_patch_with_body_only() {
        let mut server = mockito::Server::new();
        let _m = server.mock("PATCH", "/repos/owner/repo/pulls/42")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"number": 42}"#)
            .match_body(mockito::Matcher::PartialJsonString(
                r#"{"body":"updated body"}"#.to_string()
            ))
            .create();
        let client = mock_client(&server);
        client.update_pr("owner", "repo", 42, None, Some("updated body")).unwrap();
        _m.assert();
    }

    // ADR-007: inject_demo_section appends ## Demo section when none exists
    #[test]
    fn inject_demo_section_appends_when_none_exists() {
        let body = "existing description";
        let urls = vec!["https://example.com/demo.gif".to_string()];
        let result = crate::github::inject_demo_section(body, &urls);
        assert!(result.contains("## Demo"), "expected ## Demo section");
        assert!(result.contains("![Demo 1](https://example.com/demo.gif)"), "expected image markdown");
        assert!(result.starts_with("existing description"), "original body should be preserved");
    }

    // ADR-007: inject_demo_section replaces existing ## Demo section
    #[test]
    fn inject_demo_section_replaces_existing_demo_section() {
        let body = "description\n\n## Demo\n\n![Demo 1](old-url)\n";
        let urls = vec!["https://example.com/new.gif".to_string()];
        let result = crate::github::inject_demo_section(body, &urls);
        assert!(!result.contains("old-url"), "old demo url should be replaced");
        assert!(result.contains("![Demo 1](https://example.com/new.gif)"), "new url should be present");
    }

    // ADR-007: inject_demo_section with multiple URLs produces numbered entries
    #[test]
    fn inject_demo_section_numbers_multiple_demos() {
        let body = "description";
        let urls = vec![
            "https://example.com/a.gif".to_string(),
            "https://example.com/b.png".to_string(),
        ];
        let result = crate::github::inject_demo_section(body, &urls);
        assert!(result.contains("![Demo 1](https://example.com/a.gif)"));
        assert!(result.contains("![Demo 2](https://example.com/b.png)"));
    }

    // ADR-005: resolve_merge_method uses cached value on second call (no second API hit)
    #[test]
    fn resolve_merge_method_uses_cache_on_second_call() {
        let mut server = mockito::Server::new();
        // Mock set to respond exactly once — second call would 404
        let _m = server.mock("GET", "/repos/owner/repo")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"allow_squash_merge": true, "allow_merge_commit": false, "allow_rebase_merge": false}"#)
            .expect(1)
            .create();
        let client = mock_client(&server);
        let mut cache: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        let m1 = crate::github::resolve_merge_method(&client, "owner", "repo", &mut cache).unwrap();
        let m2 = crate::github::resolve_merge_method(&client, "owner", "repo", &mut cache).unwrap();
        assert_eq!(m1, "squash");
        assert_eq!(m2, "squash", "second call should return cached value");
        _m.assert(); // verifies mock was called exactly once
    }

    // ADR-005: fetch_repo_merge_methods returns "squash" when only squash is allowed
    #[test]
    fn fetch_repo_merge_methods_returns_squash_when_only_squash_allowed() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/repos/owner/repo")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"allow_squash_merge": true, "allow_merge_commit": false, "allow_rebase_merge": false}"#)
            .create();
        let client = mock_client(&server);
        let method = client.fetch_repo_merge_method("owner", "repo").unwrap();
        assert_eq!(method, "squash", "expected squash when only squash allowed");
    }

    // ADR-005: returns "merge" when only merge commit is allowed
    #[test]
    fn fetch_repo_merge_methods_returns_merge_when_only_merge_allowed() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/repos/owner/repo")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"allow_squash_merge": false, "allow_merge_commit": true, "allow_rebase_merge": false}"#)
            .create();
        let client = mock_client(&server);
        let method = client.fetch_repo_merge_method("owner", "repo").unwrap();
        assert_eq!(method, "merge", "expected merge when only merge commit allowed");
    }

    // ADR-005: returns "rebase" when only rebase is allowed
    #[test]
    fn fetch_repo_merge_methods_returns_rebase_when_only_rebase_allowed() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/repos/owner/repo")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"allow_squash_merge": false, "allow_merge_commit": false, "allow_rebase_merge": true}"#)
            .create();
        let client = mock_client(&server);
        let method = client.fetch_repo_merge_method("owner", "repo").unwrap();
        assert_eq!(method, "rebase", "expected rebase when only rebase allowed");
    }

    // ADR-005: returns "squash" (preferred) when multiple methods are allowed
    #[test]
    fn fetch_repo_merge_methods_prefers_squash_when_multiple_allowed() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/repos/owner/repo")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"allow_squash_merge": true, "allow_merge_commit": true, "allow_rebase_merge": true}"#)
            .create();
        let client = mock_client(&server);
        let method = client.fetch_repo_merge_method("owner", "repo").unwrap();
        assert_eq!(method, "squash", "expected squash preferred when multiple methods allowed");
    }

    // ADR-007: extract_github_session_from_browser_with_chrome_db errors immediately when db path absent (no Keychain call)
    #[cfg(target_os = "macos")]
    #[test]
    fn extract_github_session_with_absent_db_errors_without_keychain() {
        let result = crate::github::extract_github_session_from_browser_with_chrome_db(
            std::path::Path::new("/nonexistent/path/Cookies")
        );
        assert!(result.is_err(), "must return Err when Chrome DB path does not exist");
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("GITHUB_USER_SESSION") || msg.contains("Chrome"),
            "error must mention GITHUB_USER_SESSION or Chrome, got: {}", msg);
    }

    // ADR-007: derive_chrome_aes_key produces correct PBKDF2-SHA1 key
    #[cfg(target_os = "macos")]
    #[test]
    fn derive_chrome_aes_key_produces_known_vector() {
        let key = crate::github::derive_chrome_aes_key(b"testpassword");
        // PBKDF2-SHA1(password="testpassword", salt="saltysalt", iterations=1003, dklen=16)
        let expected: [u8; 16] = [0x6f, 0xbf, 0xc7, 0xe7, 0x02, 0x52, 0x90, 0xf4,
                                   0x7d, 0x9c, 0x2a, 0x84, 0xd6, 0x7d, 0x5f, 0xd5];
        assert_eq!(key, expected, "PBKDF2-SHA1 key must match known vector for password=testpassword");
    }

    // ADR-007: decrypt_chrome_cookie decrypts v10-prefixed AES-128-CBC value
    #[cfg(target_os = "macos")]
    #[test]
    fn decrypt_chrome_cookie_decrypts_aes_cbc_value() {
        let key = crate::github::derive_chrome_aes_key(b"testpassword");
        // v10 + 16-space IV + AES-128-CBC ciphertext of "abc123\x0a*9" (PKCS7 padded to 16 bytes)
        // Produced by: openssl enc -aes-128-cbc -K 6fbfc7e7025290f47d9c2a84d67d5fd5 -iv 20*16 -nosalt
        // v10 prefix + ciphertext (IV is always 16 spaces, hardcoded, not stored)
        let encrypted: Vec<u8> = vec![
            0x76, 0x31, 0x30, // "v10"
            0x78, 0xb5, 0xed, 0x43, 0x5d, 0xa3, 0xdd, 0x82, // ciphertext (16 bytes)
            0x11, 0xaa, 0x51, 0xd4, 0xc1, 0x47, 0x1f, 0x01,
        ];
        let result = crate::github::decrypt_chrome_cookie(&encrypted, &key).unwrap();
        assert_eq!(result, "abc123", "decrypted value must equal original plaintext");
    }

    // ADR-007: read_chrome_user_session_encrypted reads blob from Chrome cookie SQLite schema
    #[cfg(target_os = "macos")]
    #[test]
    fn read_chrome_user_session_encrypted_reads_from_sqlite() {
        use tempfile::NamedTempFile;
        use rusqlite::Connection;
        let f = NamedTempFile::new().unwrap();
        let conn = Connection::open(f.path()).unwrap();
        conn.execute_batch("CREATE TABLE cookies (host_key TEXT, name TEXT, encrypted_value BLOB)").unwrap();
        conn.execute(
            "INSERT INTO cookies (host_key, name, encrypted_value) VALUES ('github.com', 'user_session', ?1)",
            rusqlite::params![b"testblob".as_ref()],
        ).unwrap();
        let blob = crate::github::read_chrome_user_session_encrypted(f.path()).unwrap();
        assert_eq!(blob, b"testblob", "must return the encrypted_value blob from cookies table");
    }

    // ADR-007: parse_upload_token extracts token from HTML
    #[test]
    fn parse_upload_token_extracts_token_from_html() {
        let html = r#"<html><head>"uploadToken":"tok123"</head></html>"#;
        let token = crate::github::parse_upload_token(html).unwrap();
        assert_eq!(token, "tok123");
    }

    // ADR-007: parse_upload_token errors on missing token
    #[test]
    fn parse_upload_token_errors_on_missing() {
        let html = "<html><body>no token here</body></html>";
        assert!(crate::github::parse_upload_token(html).is_err());
    }

    // ADR-007: parse_upload_policy_response extracts upload fields
    #[test]
    fn parse_upload_policy_response_extracts_fields() {
        let json = r#"{
            "upload_url": "https://s3.example.com/upload",
            "asset": {"id": 42, "href": "https://github.com/user-attachments/assets/abc"},
            "form": {"key": "val1", "Content-Type": "image/png"},
            "asset_upload_authenticity_token": "auth-tok-xyz"
        }"#;
        let policy = crate::github::parse_upload_policy_response(json).unwrap();
        assert_eq!(policy.upload_url, "https://s3.example.com/upload");
        assert_eq!(policy.asset_id, 42);
        assert_eq!(policy.asset_href, "https://github.com/user-attachments/assets/abc");
        assert_eq!(policy.asset_upload_authenticity_token, "auth-tok-xyz");
        assert_eq!(policy.form_fields.get("key").map(|s| s.as_str()), Some("val1"));
    }

    fn pr_mock_with_mergeable(server: &mut mockito::Server, mergeable: &str) {
        server.mock("GET", "/repos/owner/repo/pulls/1")
            .with_status(200).with_header("content-type", "application/json")
            .with_body(format!(r#"{{"number":1,"title":"t","draft":false,"head":{{"ref":"b"}},"base":{{"ref":"main"}},"user":{{"login":"author"}},"mergeable":{}}}"#, mergeable))
            .create();
        server.mock("GET", "/repos/owner/repo/commits/b/check-runs")
            .with_status(200).with_header("content-type", "application/json")
            .with_body(r#"{"check_runs":[]}"#).create();
        server.mock("GET", "/repos/owner/repo/branches/b/protection")
            .with_status(200).with_header("content-type", "application/json")
            .with_body(r#"{"required_status_checks":{"contexts":[]}}"#).create();
        server.mock("GET", "/repos/owner/repo/pulls/1/reviews?per_page=100&page=1")
            .with_status(200).with_header("content-type", "application/json").with_body(r#"[]"#).create();
        server.mock("GET", "/repos/owner/repo/pulls/1/comments?per_page=100&page=1")
            .with_status(200).with_header("content-type", "application/json").with_body(r#"[]"#).create();
        server.mock("GET", "/repos/owner/repo/issues/1/comments?per_page=100&page=1")
            .with_status(200).with_header("content-type", "application/json").with_body(r#"[]"#).create();
    }

    // ADR-006: mergeable=false sets has_merge_conflict=true
    #[test]
    fn fetch_pr_sets_has_merge_conflict_when_mergeable_false() {
        let mut server = mockito::Server::new();
        pr_mock_with_mergeable(&mut server, "false");
        let pr = mock_client(&server).fetch_pr("owner", "repo", 1).unwrap();
        assert!(pr.has_merge_conflict, "expected has_merge_conflict=true when mergeable=false");
    }

    // ADR-006: mergeable=true sets has_merge_conflict=false
    #[test]
    fn fetch_pr_clears_has_merge_conflict_when_mergeable_true() {
        let mut server = mockito::Server::new();
        pr_mock_with_mergeable(&mut server, "true");
        let pr = mock_client(&server).fetch_pr("owner", "repo", 1).unwrap();
        assert!(!pr.has_merge_conflict, "expected has_merge_conflict=false when mergeable=true");
    }

    // ADR-006: mergeable=null (not computed) sets has_merge_conflict=false
    #[test]
    fn fetch_pr_clears_has_merge_conflict_when_mergeable_null() {
        let mut server = mockito::Server::new();
        pr_mock_with_mergeable(&mut server, "null");
        let pr = mock_client(&server).fetch_pr("owner", "repo", 1).unwrap();
        assert!(!pr.has_merge_conflict, "expected has_merge_conflict=false when mergeable=null");
    }

    #[test]
    fn fetch_pr_errors_when_pr_author_missing() {
        let mut server = mockito::Server::new();
        server.mock("GET", "/repos/owner/repo/pulls/99").with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"number": 99, "title": "t", "draft": false, "head": {"ref": "branch", "sha": "abc"}, "base": {"ref": "main"}}"#)
            .create();
        server.mock("GET", "/repos/owner/repo/commits/branch/check-runs").with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"check_runs": []}"#).create();
        server.mock("GET", "/repos/owner/repo/branches/main/protection").with_status(404).create();
        server.mock("GET", "/repos/owner/repo/commits/abc/statuses").with_status(200)
            .with_header("content-type", "application/json").with_body("[]").create();
        server.mock("GET", "/repos/owner/repo/pulls/99/reviews?per_page=100&page=1").with_status(200)
            .with_header("content-type", "application/json").with_body("[]").create();
        server.mock("GET", "/repos/owner/repo/pulls/99/requested_reviewers").with_status(200)
            .with_header("content-type", "application/json").with_body(r#"{"users":[],"teams":[]}"#).create();
        server.mock("GET", "/repos/owner/repo/issues/99/comments?per_page=100&page=1").with_status(200)
            .with_header("content-type", "application/json").with_body("[]").create();
        server.mock("GET", "/repos/owner/repo/pulls/99/comments?per_page=100&page=1").with_status(200)
            .with_header("content-type", "application/json").with_body("[]").create();
        let result = mock_client(&server).fetch_pr("owner", "repo", 99);
        assert!(result.is_err(), "fetch_pr must error when user.login is absent");
        assert!(result.unwrap_err().to_string().contains("could not determine PR author"),
            "error must mention 'could not determine PR author'");
    }

    #[test]
    fn detect_repo_works_from_worktree_subdirectory() {
        use std::fs;
        use tempfile::TempDir;
        let base = TempDir::new().unwrap();
        let repo = base.path().join("myrepo");
        fs::create_dir_all(&repo).unwrap();
        std::process::Command::new("git").args(["init"]).current_dir(&repo).output().unwrap();
        std::process::Command::new("git")
            .args(["remote", "add", "origin", "git@github.com:owner/repo.git"])
            .current_dir(&repo).output().unwrap();
        // Simulate calling from a worktree path that is a subdirectory sibling, not the repo root
        let worktree = base.path().join("myrepo-worktrees").join("feat-branch");
        fs::create_dir_all(&worktree).unwrap();
        // worktree has no git config of its own — detect_repo must use main repo root
        let result = crate::github::detect_repo_with_cwd(&repo);
        assert_eq!(result, Some(("owner".to_string(), "repo".to_string())),
            "detect_repo must return owner/repo from main repo root");
        // Now verify it returns None from a path with no git repo
        let result_from_worktree_sibling = crate::github::detect_repo_with_cwd(&worktree);
        assert!(result_from_worktree_sibling.is_none(),
            "detect_repo must return None from a non-git path: {:?}", result_from_worktree_sibling);
    }
}
