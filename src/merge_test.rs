#[cfg(test)]
mod tests {
    #[test]
    fn merge_governs_check_merge_base_errors_when_empty_with_downstream() {
        let result = crate::merge::check_merge_base("", true);
        assert!(result.is_err(), "merge::check_merge_base must error when base empty and has downstream");
        assert!(
            result.unwrap_err().to_string().contains("could not determine merge base"),
            "error must mention 'could not determine merge base'"
        );
    }

    #[test]
    fn merge_governs_resolve_merge_base_prefers_fetched() {
        let result = crate::merge::resolve_merge_base("fetched-sha", "stored-sha");
        assert_eq!(result, "fetched-sha", "merge::resolve_merge_base must prefer fetched when non-empty");
    }
}
