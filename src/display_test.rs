#[cfg(test)]
mod tests {
    #[test]
    fn display_module_governs_format_watch_initial_state() {
        let out = crate::display::format_watch_initial_state(5, "my PR", &[], false, None, "");
        assert!(out.contains("ready"), "got: {}", out);
    }

    #[test]
    fn display_module_governs_format_single_pr_status() {
        let out = crate::display::format_single_pr_status(7, &[], None);
        assert!(out.contains("ready"), "got: {}", out);
    }

    #[test]
    fn display_module_governs_format_conflict_hint() {
        let prs: std::collections::HashMap<u64, crate::store::PrCache> = std::collections::HashMap::new();
        let hint = crate::display::format_conflict_hint("feat/x", &prs);
        assert!(hint.is_empty());
    }
}
