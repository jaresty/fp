#[cfg(test)]
mod tests {
    use crate::store::{Store, TrackedPr};
    use tempfile::tempdir;

    fn make_store() -> (Store, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path());
        (store, dir)
    }

    // D1/store: empty store loads with no PRs
    #[test]
    fn empty_store_has_no_prs() {
        let (store, _dir) = make_store();
        let state = store.load().unwrap();
        assert!(state.prs.is_empty());
    }

    // store: tracked PR is persisted and retrievable
    #[test]
    fn tracked_pr_is_stored() {
        let (store, _dir) = make_store();
        store.track(TrackedPr { number: 42, title: "my pr".into(), branch: "fix/foo".into() }).unwrap();
        let state = store.load().unwrap();
        assert!(state.prs.contains_key(&42));
        assert_eq!(state.prs[&42].title, "my pr");
    }

    // store: untrack removes PR
    #[test]
    fn untrack_removes_pr() {
        let (store, _dir) = make_store();
        store.track(TrackedPr { number: 42, title: "my pr".into(), branch: "fix/foo".into() }).unwrap();
        store.untrack(42).unwrap();
        let state = store.load().unwrap();
        assert!(!state.prs.contains_key(&42));
    }

    // store: tracking multiple PRs
    #[test]
    fn multiple_prs_tracked() {
        let (store, _dir) = make_store();
        store.track(TrackedPr { number: 1, title: "pr1".into(), branch: "b1".into() }).unwrap();
        store.track(TrackedPr { number: 2, title: "pr2".into(), branch: "b2".into() }).unwrap();
        let state = store.load().unwrap();
        assert_eq!(state.prs.len(), 2);
    }

}
