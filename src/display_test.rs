#[cfg(test)]
mod tests {
    #[test]
    fn display_module_governs_format_watch_initial_state() {
        let out = crate::display::format_watch_initial_state(5, "my PR", &[], false, None, "");
        assert!(out.contains("ready"), "got: {}", out);
    }

    #[test]
    fn display_module_governs_format_single_pr_status() {
        let out = crate::display::format_single_pr_status(7, &[], None, false, false, false);
        assert!(out.contains("ready"), "got: {}", out);
    }

    #[test]
    fn display_governs_format_single_pr_status_shows_closed_tag() {
        let out = crate::display::format_single_pr_status(8, &[], None, true, false, false);
        assert!(out.contains("[closed]"), "must show [closed] when is_closed=true, got: {}", out);
    }

    #[test]
    fn display_governs_format_single_pr_status_shows_merged_tag() {
        let out = crate::display::format_single_pr_status(9, &[], None, false, true, false);
        assert!(out.contains("[merged]"), "must show [merged] when is_merged=true, got: {}", out);
    }

    #[test]
    fn display_governs_format_single_pr_status_shows_draft_tag() {
        let out = crate::display::format_single_pr_status(10, &[], None, false, false, true);
        assert!(out.contains("[draft]"), "must show [draft] when draft=true, got: {}", out);
    }

    #[test]
    fn display_governs_format_single_pr_status_no_tags_when_open() {
        let out = crate::display::format_single_pr_status(11, &[], None, false, false, false);
        assert!(!out.contains("[closed]") && !out.contains("[merged]") && !out.contains("[draft]"),
            "must not show tags when open non-draft, got: {}", out);
    }

    #[test]
    fn display_module_governs_format_conflict_hint() {
        let prs: std::collections::HashMap<u64, crate::store::PrCache> = std::collections::HashMap::new();
        let hint = crate::display::format_conflict_hint("feat/x", &prs);
        assert!(hint.is_empty());
    }
}
