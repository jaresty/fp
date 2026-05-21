#[cfg(test)]
mod tests {
    use crate::feature::{
        feature_new, feature_add, feature_add_dep, feature_list, feature_list_running,
        feature_list_running_with_config, feature_status, bootstrap_pr, teardown_pr,
        health_check_branch, health_check_pid, health_check_service,
        check_conflicts, ConflictResult, PrHealthStatus,
    };
    use crate::process_store::{ProcessRecord, ProcessStateStore};
    use crate::app_config::{AppConfig, AppConfigStore};
    use crate::store::{Store, PrCache};
    use tempfile::tempdir;

    fn ps_store() -> (ProcessStateStore, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let store = ProcessStateStore::open(dir.path().join("process-state.json"));
        (store, dir)
    }

    fn app_store() -> (AppConfigStore, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let store = AppConfigStore::open(dir.path().join("config.toml"));
        (store, dir)
    }

    fn git_store(dir: &std::path::Path) -> Store {
        Store::open(dir)
    }

    fn echo_config(name: &str) -> AppConfig {
        AppConfig {
            name: name.into(),
            bootstrap: "echo bootstrap-ok".into(),
            main_worktree: None,
            teardown: "echo teardown-ok".into(),
            startup_timeout: "5s".into(),
            health_check: None,
            ephemeral: false,
        }
    }

    fn record(pr: u64, branch: &str, worktree: &str) -> ProcessRecord {
        ProcessRecord {
            pr,
            expected_branch: branch.into(),
            pid: None,
            feature_envelope: None,
            worktree: worktree.into(),
            app_config_name: None,
        }
    }

    fn ephemeral_config(name: &str) -> crate::app_config::AppConfig {
        crate::app_config::AppConfig {
            name: name.into(),
            bootstrap: "echo install".into(),
            teardown: "echo uninstall".into(),
            startup_timeout: "5s".into(),
            health_check: Some("true".into()),
            ephemeral: true,
            main_worktree: None,
        }
    }

    // D1: feature_new creates a named envelope retrievable via feature_list
    #[test]
    fn feature_governs_new_creates_envelope() {
        let (ps, _dir) = ps_store();
        feature_new(&ps, "auth-refactor").unwrap();
        let list = feature_list(&ps).unwrap();
        assert!(list.iter().any(|f| f.name == "auth-refactor"),
            "feature_new must create envelope 'auth-refactor', got: {:?}", list.iter().map(|f| &f.name).collect::<Vec<_>>());
    }

    // D2: feature_add adds PR to envelope; feature_list shows membership
    #[test]
    fn feature_governs_add_adds_pr_to_envelope() {
        let (ps, _dir) = ps_store();
        let git_dir = tempdir().unwrap();
        let store = git_store(git_dir.path());
        store.track(42).unwrap();
        store.update_cache(PrCache { number: 42, title: "T".into(), branch: "feat/x".into(), base: "main".into() }).unwrap();
        feature_new(&ps, "my-feature").unwrap();
        feature_add(&ps, &store, "my-feature", 42).unwrap();
        let list = feature_list(&ps).unwrap();
        let envelope = list.iter().find(|f| f.name == "my-feature").unwrap();
        assert!(envelope.prs.contains(&42),
            "feature_add must add PR 42 to 'my-feature', got: {:?}", envelope.prs);
    }

    // D2b: feature_add auto-tracks an untracked PR
    #[test]
    fn feature_governs_add_auto_tracks_untracked_pr() {
        let (ps, _dir) = ps_store();
        let git_dir = tempdir().unwrap();
        let store = git_store(git_dir.path());
        // PR 99 is not tracked initially
        feature_new(&ps, "my-feature").unwrap();
        feature_add(&ps, &store, "my-feature", 99).unwrap();
        let state = store.load().unwrap();
        assert!(state.tracked.contains(&99),
            "feature_add must auto-track PR 99, got tracked: {:?}", state.tracked);
    }

    // D3: feature_list returns empty list when no envelopes
    #[test]
    fn feature_governs_list_returns_empty_when_no_envelopes() {
        let (ps, _dir) = ps_store();
        let list = feature_list(&ps).unwrap();
        assert!(list.is_empty(),
            "feature_list must return empty when no envelopes, got: {:?}", list);
    }

    // D4: bootstrap_pr runs bootstrap command and records PID in process state
    #[test]
    fn feature_governs_bootstrap_pr_records_activation() {
        let (ps, _dir) = ps_store();
        let worktree = tempdir().unwrap();
        let cfg = echo_config("svc");
        bootstrap_pr(&ps, &cfg, 42, worktree.path(), "acme", "repo").unwrap();
        let state = ps.load().unwrap();
        assert!(state.records.contains_key(&42),
            "bootstrap_pr must record PR 42 in process state, got keys: {:?}", state.records.keys().collect::<Vec<_>>());
    }

    // D4b: bootstrap_pr stores expected_branch in record (from config name, not branch name here — uses worktree)
    #[test]
    fn feature_governs_bootstrap_pr_stores_worktree_path() {
        let (ps, _dir) = ps_store();
        let worktree = tempdir().unwrap();
        let cfg = echo_config("svc");
        bootstrap_pr(&ps, &cfg, 42, worktree.path(), "acme", "repo").unwrap();
        let state = ps.load().unwrap();
        assert_eq!(state.records[&42].worktree, worktree.path().to_string_lossy().as_ref(),
            "bootstrap_pr must store worktree path, got: {:?}", state.records[&42].worktree);
    }

    // D5: teardown_pr runs teardown command and removes record from process state
    #[test]
    fn feature_governs_teardown_pr_removes_activation() {
        let (ps, _dir) = ps_store();
        let worktree = tempdir().unwrap();
        let cfg = echo_config("svc");
        bootstrap_pr(&ps, &cfg, 42, worktree.path(), "acme", "repo").unwrap();
        teardown_pr(&ps, &cfg, 42, worktree.path(), "acme", "repo").unwrap();
        let state = ps.load().unwrap();
        assert!(!state.records.contains_key(&42),
            "teardown_pr must remove PR 42 from process state, got keys: {:?}", state.records.keys().collect::<Vec<_>>());
    }

    // D7: health_check_branch returns true when worktree HEAD matches expected branch
    #[test]
    fn feature_governs_health_check_branch_correct_when_head_matches() {
        let tmp = tempdir().unwrap();
        let env = [("GIT_AUTHOR_NAME","t"),("GIT_AUTHOR_EMAIL","t@t"),("GIT_COMMITTER_NAME","t"),("GIT_COMMITTER_EMAIL","t@t")];
        std::process::Command::new("git").args(["init", "-b", "feat/test"]).current_dir(tmp.path()).output().unwrap();
        let mut cmd = std::process::Command::new("git");
        cmd.args(["commit", "--allow-empty", "-m", "init"]).current_dir(tmp.path());
        for (k,v) in &env { cmd.env(k,v); }
        cmd.output().unwrap();
        let result = health_check_branch(tmp.path(), "feat/test");
        assert!(result, "health_check_branch must return true when HEAD is feat/test");
    }

    // D7b: health_check_branch returns false when worktree HEAD differs
    #[test]
    fn feature_governs_health_check_branch_wrong_when_head_differs() {
        let tmp = tempdir().unwrap();
        let env = [("GIT_AUTHOR_NAME","t"),("GIT_AUTHOR_EMAIL","t@t"),("GIT_COMMITTER_NAME","t"),("GIT_COMMITTER_EMAIL","t@t")];
        std::process::Command::new("git").args(["init", "-b", "main"]).current_dir(tmp.path()).output().unwrap();
        let mut cmd = std::process::Command::new("git");
        cmd.args(["commit", "--allow-empty", "-m", "init"]).current_dir(tmp.path());
        for (k,v) in &env { cmd.env(k,v); }
        cmd.output().unwrap();
        let result = health_check_branch(tmp.path(), "feat/other");
        assert!(!result, "health_check_branch must return false when HEAD is main, expected feat/other");
    }

    // D6: health_check_pid returns false for a dead PID
    #[test]
    fn feature_governs_health_check_pid_dead_returns_false() {
        // PID u32::MAX is virtually guaranteed to not exist
        let result = health_check_pid(u32::MAX);
        assert!(!result, "health_check_pid must return false for non-existent PID {}", u32::MAX);
    }

    // D6b: health_check_pid returns true for the current process
    #[test]
    fn feature_governs_health_check_pid_live_returns_true() {
        let pid = std::process::id();
        let result = health_check_pid(pid);
        assert!(result, "health_check_pid must return true for current PID {}", pid);
    }

    // D8: health_check_service returns true when command exits 0
    #[test]
    fn feature_governs_health_check_service_returns_true_on_exit_0() {
        let tmp = tempdir().unwrap();
        let result = health_check_service("true", tmp.path(), 42, tmp.path());
        assert!(result, "health_check_service must return true when command exits 0");
    }

    // D8b: health_check_service returns false when command exits non-0
    #[test]
    fn feature_governs_health_check_service_returns_false_on_nonzero() {
        let tmp = tempdir().unwrap();
        let result = health_check_service("false", tmp.path(), 42, tmp.path());
        assert!(!result, "health_check_service must return false when command exits non-0");
    }

    // D10: check_conflicts returns NoConflict when no other envelopes are live
    #[test]
    fn feature_governs_check_conflicts_no_conflict_when_no_live_envelopes() {
        let (ps, _dir) = ps_store();
        feature_new(&ps, "my-feature").unwrap();
        let result = check_conflicts(&ps, "my-feature").unwrap();
        assert!(matches!(result, ConflictResult::NoConflict),
            "check_conflicts must return NoConflict when nothing else is live, got: {:?}", result);
    }

    // Stage 3: feature_list_running — D11
    #[test]
    fn feature_governs_list_running_excludes_envelopes_with_no_live_pid() {
        let (ps, _dir) = ps_store();
        feature_new(&ps, "dead-feature").unwrap();
        let running = feature_list_running(&ps).unwrap();
        assert!(running.is_empty(),
            "feature_list_running must exclude envelopes with no live PIDs, got: {:?}", running.iter().map(|f| &f.name).collect::<Vec<_>>());
    }

    // Stage 3: feature_list_running — D11b
    #[test]
    fn feature_governs_list_running_includes_envelope_with_live_pid() {
        let (ps, _dir) = ps_store();
        let live_pid = std::process::id();
        let mut rec = record(77, "feat/foo", "/tmp/wt77");
        rec.feature_envelope = Some("live-feature".into());
        rec.pid = Some(live_pid);
        ps.activate(rec).unwrap();
        let mut state = ps.load().unwrap();
        state.feature_envelopes.insert("live-feature".to_string());
        ps.save_state(state).unwrap();
        let running = feature_list_running(&ps).unwrap();
        assert!(running.iter().any(|f| f.name == "live-feature"),
            "feature_list_running must include 'live-feature' with live PID, got: {:?}", running.iter().map(|f| &f.name).collect::<Vec<_>>());
    }

    // Stage 3: feature_status — D12
    #[test]
    fn feature_governs_status_returns_entry_per_pr() {
        let (ps, _dir) = ps_store();
        let (app_store, _app_dir) = app_store();
        let tmp = tempdir().unwrap();
        let env = [("GIT_AUTHOR_NAME","t"),("GIT_AUTHOR_EMAIL","t@t"),("GIT_COMMITTER_NAME","t"),("GIT_COMMITTER_EMAIL","t@t")];
        std::process::Command::new("git").args(["init", "-b", "feat/pay"]).current_dir(tmp.path()).output().unwrap();
        let mut cmd = std::process::Command::new("git");
        cmd.args(["commit", "--allow-empty", "-m", "init"]).current_dir(tmp.path());
        for (k,v) in &env { cmd.env(k,v); }
        cmd.output().unwrap();
        let live_pid = std::process::id();
        let mut rec = record(123, "feat/pay", &tmp.path().to_string_lossy());
        rec.feature_envelope = Some("auth-refactor".into());
        rec.pid = Some(live_pid);
        ps.activate(rec).unwrap();
        let mut state = ps.load().unwrap();
        state.feature_envelopes.insert("auth-refactor".to_string());
        ps.save_state(state).unwrap();
        let statuses = feature_status(&ps, &app_store, "auth-refactor").unwrap();
        assert_eq!(statuses.len(), 1,
            "feature_status must return one entry for PR 123, got: {:?}", statuses.len());
        assert_eq!(statuses[0].pr, 123);
    }

    // Stage 3: feature_status — D12b: pid_alive reflects live PID
    #[test]
    fn feature_governs_status_pid_alive_true_for_live_pid() {
        let (ps, _dir) = ps_store();
        let (app_store, _app_dir) = app_store();
        let tmp = tempdir().unwrap();
        let live_pid = std::process::id();
        let mut rec = record(123, "feat/pay", &tmp.path().to_string_lossy());
        rec.feature_envelope = Some("auth-refactor".into());
        rec.pid = Some(live_pid);
        ps.activate(rec).unwrap();
        let mut state = ps.load().unwrap();
        state.feature_envelopes.insert("auth-refactor".to_string());
        ps.save_state(state).unwrap();
        let statuses = feature_status(&ps, &app_store, "auth-refactor").unwrap();
        assert!(statuses[0].pid_alive,
            "feature_status must report pid_alive=true for live PID {}", live_pid);
    }

    // Stage 3: feature_status — D12c: branch_ok true when HEAD matches expected
    #[test]
    fn feature_governs_status_branch_ok_when_head_matches_expected() {
        let (ps, _dir) = ps_store();
        let (app_store, _app_dir) = app_store();
        let tmp = tempdir().unwrap();
        let env = [("GIT_AUTHOR_NAME","t"),("GIT_AUTHOR_EMAIL","t@t"),("GIT_COMMITTER_NAME","t"),("GIT_COMMITTER_EMAIL","t@t")];
        std::process::Command::new("git").args(["init", "-b", "feat/pay"]).current_dir(tmp.path()).output().unwrap();
        let mut cmd = std::process::Command::new("git");
        cmd.args(["commit", "--allow-empty", "-m", "init"]).current_dir(tmp.path());
        for (k,v) in &env { cmd.env(k,v); }
        cmd.output().unwrap();
        let mut rec = record(123, "feat/pay", &tmp.path().to_string_lossy());
        rec.feature_envelope = Some("auth-refactor".into());
        rec.pid = None;
        ps.activate(rec).unwrap();
        let mut state = ps.load().unwrap();
        state.feature_envelopes.insert("auth-refactor".to_string());
        ps.save_state(state).unwrap();
        let statuses = feature_status(&ps, &app_store, "auth-refactor").unwrap();
        assert!(statuses[0].branch_ok,
            "feature_status must report branch_ok=true when HEAD is feat/pay");
    }

    // D10b: check_conflicts returns Conflict when another envelope has a live record
    #[test]
    fn feature_governs_check_conflicts_conflict_when_other_envelope_live() {
        let (ps, _dir) = ps_store();
        // other-feature has PR 99 with a live PID (current process = definitely alive)
        let live_pid = std::process::id();
        let mut rec = record(99, "feat/other", "/tmp/wt");
        rec.feature_envelope = Some("other-feature".into());
        rec.pid = Some(live_pid);
        ps.activate(rec).unwrap();
        feature_new(&ps, "my-feature").unwrap();
        let result = check_conflicts(&ps, "my-feature").unwrap();
        assert!(matches!(result, ConflictResult::Conflict { .. }),
            "check_conflicts must return Conflict when other-feature has live PR, got: {:?}", result);
    }

    // Ephemeral: feature_status reports pid_alive=false and installed=true for passing health_check
    #[test]
    fn feature_governs_status_ephemeral_installed_when_health_check_passes() {
        let (ps, _dir) = ps_store();
        let (app_store, _app_dir) = app_store();
        let wt = tempdir().unwrap();
        app_store.save_app_config(ephemeral_config("my-ext")).unwrap();
        let mut rec = record(789, "feat/ext", &wt.path().to_string_lossy());
        rec.feature_envelope = Some("ext-feature".into());
        rec.app_config_name = Some("my-ext".into());
        // no pid — ephemeral
        ps.activate(rec).unwrap();
        let mut state = ps.load().unwrap();
        state.feature_envelopes.insert("ext-feature".to_string());
        ps.save_state(state).unwrap();
        let statuses = feature_status(&ps, &app_store, "ext-feature").unwrap();
        assert_eq!(statuses.len(), 1);
        assert!(!statuses[0].pid_alive, "ephemeral app must have pid_alive=false");
        assert_eq!(statuses[0].service_healthy, Some(true),
            "ephemeral app with passing health_check must report service_healthy=true");
    }

    // Ephemeral: feature_list_running includes envelope when ephemeral health_check passes
    #[test]
    fn feature_governs_list_running_includes_ephemeral_when_health_check_passes() {
        let (ps, _dir) = ps_store();
        let (app_store, _app_dir) = app_store();
        let wt = tempdir().unwrap();
        app_store.save_app_config(ephemeral_config("my-ext")).unwrap();
        let mut rec = record(789, "feat/ext", &wt.path().to_string_lossy());
        rec.feature_envelope = Some("ext-feature".into());
        rec.app_config_name = Some("my-ext".into());
        ps.activate(rec).unwrap();
        let mut state = ps.load().unwrap();
        state.feature_envelopes.insert("ext-feature".to_string());
        ps.save_state(state).unwrap();
        let running = feature_list_running_with_config(&ps, &app_store).unwrap();
        assert!(running.iter().any(|f| f.name == "ext-feature"),
            "feature_list_running must include ephemeral envelope with passing health_check, got: {:?}",
            running.iter().map(|f| &f.name).collect::<Vec<_>>());
    }

    // envelope_deps: feature_add_dep stores app config name in envelope_deps
    #[test]
    fn feature_governs_add_dep_stores_config_in_envelope_deps() {
        let (ps, _dir) = ps_store();
        feature_new(&ps, "my-feature").unwrap();
        feature_add_dep(&ps, "my-feature", "notifications-svc").unwrap();
        let state = ps.load().unwrap();
        let deps = state.envelope_deps.get("my-feature").cloned().unwrap_or_default();
        assert!(deps.contains(&"notifications-svc".to_string()),
            "envelope_deps must contain 'notifications-svc', got: {:?}", deps);
    }

    // feature_up starts main-worktree instance for dep slot with no live PR
    #[test]
    fn feature_governs_up_starts_main_worktree_for_dep_with_no_pr() {
        let (ps, _dir) = ps_store();
        let (app_store, _app_dir) = app_store();
        let main_wt = tempdir().unwrap();
        let mut cfg = echo_config("notifications-svc");
        cfg.main_worktree = Some(main_wt.path().to_string_lossy().to_string());
        app_store.save_app_config(cfg).unwrap();
        feature_new(&ps, "my-feature").unwrap();
        feature_add_dep(&ps, "my-feature", "notifications-svc").unwrap();
        crate::feature::feature_up(&ps, &app_store, "my-feature").unwrap();
        let state = ps.load().unwrap();
        // sentinel PR 0 should be recorded for notifications-svc main-worktree instance
        assert!(state.records.contains_key(&0),
            "feature_up must record sentinel PR 0 for main-worktree dep instance, got keys: {:?}",
            state.records.keys().collect::<Vec<_>>());
        assert_eq!(state.records[&0].worktree, main_wt.path().to_string_lossy().as_ref(),
            "sentinel record must use main_worktree path");
    }
}
