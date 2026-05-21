#[cfg(test)]
mod tests {
    use crate::process_store::{ProcessRecord, ProcessStateStore};
    use tempfile::tempdir;

    fn make_store() -> (ProcessStateStore, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let store = ProcessStateStore::open(dir.path());
        (store, dir)
    }

    fn record(pr: u64, branch: &str) -> ProcessRecord {
        ProcessRecord {
            pr,
            expected_branch: branch.into(),
            pid: Some(12345),
            feature_envelope: None,
            worktree: "/tmp/worktree".into(),
            app_config_names: vec![],
        }
    }

    // D2: load returns empty state when no file exists
    #[test]
    fn process_store_governs_load_returns_empty_when_no_file() {
        let (store, _dir) = make_store();
        let state = store.load().unwrap();
        assert!(state.records.is_empty(),
            "process_store::load must return empty records when no file exists, got: {:?}", state.records);
    }

    // D3: activate persists entry; subsequent load contains it
    #[test]
    fn process_store_governs_activate_persists_entry() {
        let (store, _dir) = make_store();
        store.activate(record(42, "feat/payments")).unwrap();
        let state = store.load().unwrap();
        assert!(state.records.contains_key(&42),
            "process_store::activate must persist PR 42, got keys: {:?}", state.records.keys().collect::<Vec<_>>());
    }

    // D4: loaded entry has correct expected_branch
    #[test]
    fn process_store_governs_activate_stores_expected_branch() {
        let (store, _dir) = make_store();
        store.activate(record(42, "feat/payments")).unwrap();
        let state = store.load().unwrap();
        assert_eq!(state.records[&42].expected_branch, "feat/payments",
            "process_store::activate must store expected_branch, got: {:?}", state.records[&42].expected_branch);
    }

    // D5: loaded entry has correct pid
    #[test]
    fn process_store_governs_activate_stores_pid() {
        let (store, _dir) = make_store();
        store.activate(record(42, "feat/payments")).unwrap();
        let state = store.load().unwrap();
        assert_eq!(state.records[&42].pid, Some(12345),
            "process_store::activate must store pid, got: {:?}", state.records[&42].pid);
    }

    // D6: loaded entry has correct feature_envelope
    #[test]
    fn process_store_governs_activate_stores_feature_envelope() {
        let (store, _dir) = make_store();
        let mut rec = record(42, "feat/payments");
        rec.feature_envelope = Some("auth-refactor".into());
        store.activate(rec).unwrap();
        let state = store.load().unwrap();
        assert_eq!(state.records[&42].feature_envelope, Some("auth-refactor".into()),
            "process_store::activate must store feature_envelope, got: {:?}", state.records[&42].feature_envelope);
    }

    // D7: deactivate removes entry; subsequent load does not contain it
    #[test]
    fn process_store_governs_deactivate_removes_entry() {
        let (store, _dir) = make_store();
        store.activate(record(42, "feat/payments")).unwrap();
        store.deactivate(42).unwrap();
        let state = store.load().unwrap();
        assert!(!state.records.contains_key(&42),
            "process_store::deactivate must remove PR 42, got keys: {:?}", state.records.keys().collect::<Vec<_>>());
    }

    // D8: open derives path as <git_dir>/fp/process-state.json
    #[test]
    fn process_store_governs_open_path_is_under_git_dir() {
        let dir = tempdir().unwrap();
        let store = ProcessStateStore::open(dir.path());
        let expected = dir.path().join("fp").join("process-state.json");
        assert_eq!(store.path, expected,
            "ProcessStateStore::open must place state at <git_dir>/fp/process-state.json");
    }

    // multiple activations — second overwrites first for same PR
    #[test]
    fn process_store_governs_activate_overwrites_existing_entry() {
        let (store, _dir) = make_store();
        store.activate(record(42, "feat/old-branch")).unwrap();
        let mut rec2 = record(42, "feat/new-branch");
        rec2.pid = Some(99999);
        store.activate(rec2).unwrap();
        let state = store.load().unwrap();
        assert_eq!(state.records[&42].expected_branch, "feat/new-branch",
            "second activate must overwrite branch, got: {:?}", state.records[&42].expected_branch);
        assert_eq!(state.records[&42].pid, Some(99999),
            "second activate must overwrite pid, got: {:?}", state.records[&42].pid);
    }

    // multiple PRs coexist
    #[test]
    fn process_store_governs_multiple_prs_coexist() {
        let (store, _dir) = make_store();
        store.activate(record(1, "feat/a")).unwrap();
        store.activate(record(2, "feat/b")).unwrap();
        let state = store.load().unwrap();
        assert_eq!(state.records.len(), 2,
            "process_store must hold multiple PR records, got: {:?}", state.records.len());
    }
}
