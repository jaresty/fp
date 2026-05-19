#[cfg(test)]
mod tests {
    use crate::store::{Store, PrCache};
    use tempfile::tempdir;

    fn make_store() -> (Store, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path());
        (store, dir)
    }

    fn cache(number: u64, title: &str, branch: &str, base: &str) -> PrCache {
        PrCache { number, title: title.into(), branch: branch.into(), base: base.into() }
    }

    // D1/store: empty store loads with no PRs
    #[test]
    fn empty_store_has_no_prs() {
        let (store, _dir) = make_store();
        let state = store.load().unwrap();
        assert!(state.tracked.is_empty());
    }

    // store: tracked PR is persisted and retrievable
    #[test]
    fn tracked_pr_is_stored() {
        let (store, _dir) = make_store();
        store.track(42).unwrap();
        store.update_cache(cache(42, "my pr", "fix/foo", "")).unwrap();
        let state = store.load().unwrap();
        assert!(state.tracked.contains(&42));
        assert_eq!(state.cache[&42].title, "my pr");
    }

    // store: untrack removes PR
    #[test]
    fn untrack_removes_pr() {
        let (store, _dir) = make_store();
        store.track(42).unwrap();
        store.update_cache(cache(42, "my pr", "fix/foo", "")).unwrap();
        store.untrack(42).unwrap();
        let state = store.load().unwrap();
        assert!(!state.tracked.contains(&42));
        assert!(!state.cache.contains_key(&42));
    }

    // store: base field is stored and retrievable
    #[test]
    fn track_stores_base_field() {
        let (store, _dir) = make_store();
        store.track(99).unwrap();
        store.update_cache(cache(99, "pr", "feat/x", "develop")).unwrap();
        let state = store.load().unwrap();
        assert_eq!(state.cache[&99].base, "develop", "expected base 'develop', got '{}'", state.cache[&99].base);
    }

    // store: tracking multiple PRs
    #[test]
    fn multiple_prs_tracked() {
        let (store, _dir) = make_store();
        store.track(1).unwrap();
        store.update_cache(cache(1, "pr1", "b1", "")).unwrap();
        store.track(2).unwrap();
        store.update_cache(cache(2, "pr2", "b2", "")).unwrap();
        let state = store.load().unwrap();
        assert_eq!(state.tracked.len(), 2);
    }

    // store: old-format state.json (prs key) is migrated to tracked + cache
    #[test]
    fn legacy_format_migrates_to_tracked_and_cache() {
        let dir = tempdir().unwrap();
        let fp_dir = dir.path().join("fp");
        std::fs::create_dir_all(&fp_dir).unwrap();
        let state_path = fp_dir.join("state.json");
        std::fs::write(&state_path, r#"{"prs":{"42":{"number":42,"title":"my pr","branch":"fix/foo","base":"main"}},"cached_merge_methods":{}}"#).unwrap();
        let store = Store::open(dir.path());
        let state = store.load().unwrap();
        assert!(state.tracked.contains(&42), "legacy prs must be migrated to tracked set, got tracked={:?}", state.tracked);
        assert_eq!(state.cache[&42].title, "my pr", "legacy pr title must appear in cache");
    }

}
