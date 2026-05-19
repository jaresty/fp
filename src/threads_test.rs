#[cfg(test)]
mod tests {
    #[test]
    fn display_governs_format_open_threads_empty() {
        let out = crate::display::format_open_threads(5, &[], false);
        assert!(
            out.contains("No open threads"),
            "display::format_open_threads empty must say 'No open threads': {}",
            out
        );
    }

    #[test]
    fn display_governs_fetch_open_threads_filters_resolved() {
        use crate::model::{Thread, ThreadState};
        let threads = vec![
            Thread { id: 1, body: "open".into(), state: ThreadState::Open, file: None, line: None, author: String::new(), replies: vec![] },
            Thread { id: 2, body: "resolved".into(), state: ThreadState::Resolved, file: None, line: None, author: String::new(), replies: vec![] },
        ];
        let open = crate::display::fetch_open_threads(&threads);
        assert_eq!(open.len(), 1, "display::fetch_open_threads must exclude resolved threads");
    }

    #[test]
    fn model_governs_parse_resolved_threads_returns_body() {
        let json = r#"{"data":{"repository":{"pullRequest":{"commits":{"nodes":[]},"reviewThreads":{"nodes":[{"isResolved":true,"resolvedBy":{"login":"alice"},"comments":{"nodes":[{"body":"fix this","path":"src/main.rs","line":10,"createdAt":"2024-01-01T00:00:00Z"}]}}]}}}}}"#;
        let threads = crate::model::parse_resolved_review_threads_from_graphql(json).unwrap();
        assert_eq!(threads.len(), 1, "model::parse_resolved_review_threads_from_graphql must parse one resolved thread");
        assert_eq!(threads[0].body, "fix this");
    }
}
