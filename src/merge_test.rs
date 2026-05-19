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

    #[test]
    fn merge_governs_resolve_track_branch_uses_fetched_when_explicit_absent() {
        let result = crate::merge::resolve_track_branch(None, Some("feature/foo".to_string()), 99);
        assert!(result.is_ok(), "merge::resolve_track_branch must return Ok when fetched is present");
        assert_eq!(result.unwrap(), "feature/foo", "merge::resolve_track_branch must return fetched branch");
    }

    #[test]
    fn merge_governs_resolve_track_branch_errors_when_both_absent() {
        let err = crate::merge::resolve_track_branch(None, None, 99).unwrap_err();
        assert!(
            err.to_string().contains("fp track 99"),
            "merge::resolve_track_branch error must contain corrective command: {}",
            err
        );
    }

    #[test]
    fn merge_governs_resolve_track_branch_prefers_explicit() {
        let result = crate::merge::resolve_track_branch(
            Some("explicit-branch".to_string()),
            Some("fetched-branch".to_string()),
            99,
        );
        assert_eq!(result.unwrap(), "explicit-branch", "merge::resolve_track_branch must prefer explicit over fetched");
    }
}
