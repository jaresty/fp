#[cfg(test)]
mod tests {
    use crate::feature::{
        feature_new, feature_add, feature_add_dep, feature_list, feature_list_running,
        feature_list_running_with_config, feature_status, bootstrap_pr, teardown_pr,
        health_check_branch, health_check_pid, health_check_service,
        check_conflicts, ConflictResult, PrHealthStatus, resolve_worktree,
    };
    use crate::process_store::{ProcessRecord, ProcessStateStore};
    use crate::app_config::{AppConfig, AppConfigStore};
    use crate::store::{Store, PrCache};
    use tempfile::tempdir;

    fn ps_store() -> (ProcessStateStore, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let store = ProcessStateStore::open(dir.path());
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
            main_worktree: None, setup: None,
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
            feature_envelopes: vec![], feature_envelope: None,
            worktree: worktree.into(),
            app_config_names: vec![],
        }
    }

    /// Creates a git repo at `repo_path` and an fp-standard worktree at
    /// `worktree_path(repo_path, branch)`. Returns the derived worktree path.
    fn setup_git_worktree(repo_path: &std::path::Path, branch: &str) -> std::path::PathBuf {
        use std::process::Command;
        Command::new("git").args(["init", "-b", "main", repo_path.to_str().unwrap()]).output().unwrap();
        for arg in &[["config","user.email","t@t.com"],["config","user.name","T"]] {
            Command::new("git").args(["-C", repo_path.to_str().unwrap()]).args(arg).output().unwrap();
        }
        std::fs::write(repo_path.join("f"), "x").unwrap();
        Command::new("git").args(["-C", repo_path.to_str().unwrap(), "add", "."]).output().unwrap();
        Command::new("git").args(["-C", repo_path.to_str().unwrap(), "commit", "--allow-empty", "-m", "init"]).output().unwrap();
        Command::new("git").args(["-C", repo_path.to_str().unwrap(), "checkout", "-b", branch]).output().unwrap();
        Command::new("git").args(["-C", repo_path.to_str().unwrap(), "checkout", "main"]).output().unwrap();
        let wt = crate::worktree::worktree_path(repo_path, branch);
        std::fs::create_dir_all(wt.parent().unwrap()).unwrap();
        Command::new("git").args(["-C", repo_path.to_str().unwrap(), "worktree", "add", wt.to_str().unwrap(), branch]).output().unwrap();
        wt
    }

    fn ephemeral_config(name: &str) -> crate::app_config::AppConfig {
        crate::app_config::AppConfig {
            name: name.into(),
            bootstrap: "echo install".into(),
            teardown: "echo uninstall".into(),
            startup_timeout: "5s".into(),
            health_check: Some("true".into()),
            ephemeral: true,
            main_worktree: None, setup: None,
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
        feature_add(&ps, &store, "my-feature", 42, &[]).unwrap();
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
        feature_add(&ps, &store, "my-feature", 99, &[]).unwrap();
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

    // D5: teardown_pr runs teardown command and clears pid but preserves record
    #[test]
    fn feature_governs_teardown_pr_clears_pid_preserves_record() {
        let (ps, _dir) = ps_store();
        let worktree = tempdir().unwrap();
        let cfg = echo_config("svc");
        bootstrap_pr(&ps, &cfg, 42, worktree.path(), "acme", "repo").unwrap();
        teardown_pr(&ps, &cfg, 42, worktree.path(), "acme", "repo").unwrap();
        let state = ps.load().unwrap();
        assert!(state.records.contains_key(&42),
            "teardown_pr must preserve record for PR 42 (for re-up), got keys: {:?}", state.records.keys().collect::<Vec<_>>());
        assert!(state.records[&42].pid.is_none(),
            "teardown_pr must clear pid for PR 42");
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
        assert_eq!(result, Some(true), "health_check_branch must return Some(true) when HEAD is feat/test");
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
        assert_eq!(result, Some(false), "health_check_branch must return Some(false) when HEAD is main, expected feat/other");
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
        let statuses = feature_status(&ps, &app_store, "auth-refactor", std::path::Path::new(".")).unwrap();
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
        let statuses = feature_status(&ps, &app_store, "auth-refactor", std::path::Path::new(".")).unwrap();
        assert!(statuses[0].pid_alive,
            "feature_status must report pid_alive=true for live PID {}", live_pid);
    }

    // Stage 3: feature_status — D12c: branch_ok true when HEAD matches expected
    #[test]
    fn feature_governs_status_branch_ok_when_head_matches_expected() {
        let (ps, _dir) = ps_store();
        let (app_store, _app_dir) = app_store();
        let tmp = tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        setup_git_worktree(&repo, "feat/pay");
        let mut rec = record(123, "feat/pay", "");
        rec.feature_envelope = Some("auth-refactor".into());
        rec.pid = None;
        ps.activate(rec).unwrap();
        let mut state = ps.load().unwrap();
        state.feature_envelopes.insert("auth-refactor".to_string());
        ps.save_state(state).unwrap();
        let statuses = feature_status(&ps, &app_store, "auth-refactor", &repo).unwrap();
        assert_eq!(statuses[0].branch_ok, Some(true),
            "feature_status must report branch_ok=Some(true) when HEAD is feat/pay");
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
        let tmp = tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        setup_git_worktree(&repo, "feat/ext");
        app_store.save_app_config(ephemeral_config("my-ext")).unwrap();
        let mut rec = record(789, "feat/ext", "");
        rec.feature_envelope = Some("ext-feature".into());
        rec.app_config_names = vec!["my-ext".into()];
        ps.activate(rec).unwrap();
        let mut state = ps.load().unwrap();
        state.feature_envelopes.insert("ext-feature".to_string());
        ps.save_state(state).unwrap();
        let statuses = feature_status(&ps, &app_store, "ext-feature", &repo).unwrap();
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
        let tmp = tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        setup_git_worktree(&repo, "feat/ext");
        app_store.save_app_config(ephemeral_config("my-ext")).unwrap();
        let mut rec = record(789, "feat/ext", "");
        rec.feature_envelope = Some("ext-feature".into());
        rec.app_config_names = vec!["my-ext".into()];
        ps.activate(rec).unwrap();
        let mut state = ps.load().unwrap();
        state.feature_envelopes.insert("ext-feature".to_string());
        ps.save_state(state).unwrap();
        let running = feature_list_running_with_config(&ps, &app_store, &repo).unwrap();
        assert!(running.iter().any(|f| f.name == "ext-feature"),
            "feature_list_running must include ephemeral envelope with passing health_check, got: {:?}",
            running.iter().map(|f| &f.name).collect::<Vec<_>>());
    }

    // Ephemeral: PrHealthStatus.ephemeral is true when app config is ephemeral
    #[test]
    fn feature_governs_status_ephemeral_sets_ephemeral_true() {
        let (ps, _dir) = ps_store();
        let (app_store, _app_dir) = app_store();
        let tmp = tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        setup_git_worktree(&repo, "feat/ext");
        app_store.save_app_config(ephemeral_config("my-ext")).unwrap();
        let mut rec = record(789, "feat/ext", "");
        rec.feature_envelope = Some("ext-feature".into());
        rec.app_config_names = vec!["my-ext".into()];
        ps.activate(rec).unwrap();
        let mut state = ps.load().unwrap();
        state.feature_envelopes.insert("ext-feature".to_string());
        ps.save_state(state).unwrap();
        let statuses = feature_status(&ps, &app_store, "ext-feature", &repo).unwrap();
        assert_eq!(statuses.len(), 1);
        assert!(statuses[0].ephemeral, "PrHealthStatus.ephemeral must be true for ephemeral app");
    }

    // Ephemeral: feature_list_running includes envelope even when no health_check configured
    #[test]
    fn feature_governs_list_running_includes_ephemeral_without_health_check() {
        let (ps, _dir) = ps_store();
        let (app_store, _app_dir) = app_store();
        let tmp = tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        setup_git_worktree(&repo, "feat/ext");
        app_store.save_app_config(crate::app_config::AppConfig {
            name: "my-ext".into(),
            bootstrap: "echo install".into(),
            teardown: "echo uninstall".into(),
            startup_timeout: "5s".into(),
            health_check: None,
            ephemeral: true,
            main_worktree: None, setup: None,
        }).unwrap();
        let mut rec = record(789, "feat/ext", "");
        rec.feature_envelope = Some("ext-feature".into());
        rec.app_config_names = vec!["my-ext".into()];
        ps.activate(rec).unwrap();
        let mut state = ps.load().unwrap();
        state.feature_envelopes.insert("ext-feature".to_string());
        ps.save_state(state).unwrap();
        let running = feature_list_running_with_config(&ps, &app_store, &repo).unwrap();
        assert!(running.iter().any(|f| f.name == "ext-feature"),
            "feature_list_running must include ephemeral envelope even without health_check, got: {:?}",
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
    fn feature_governs_up_stores_dep_in_dep_records_not_pr0() {
        let (ps, _dir) = ps_store();
        let (app_store, _app_dir) = app_store();
        let repo = tempdir().unwrap();
        let mut cfg = echo_config("notifications-svc");
        cfg.main_worktree = None;
        app_store.save_app_config(cfg).unwrap();
        feature_new(&ps, "my-feature").unwrap();
        feature_add_dep(&ps, "my-feature", "notifications-svc").unwrap();
        crate::feature::feature_up(&ps, &app_store, "my-feature", repo.path()).unwrap();
        let state = ps.load().unwrap();
        let key = "my-feature:notifications-svc";
        assert!(state.dep_records.contains_key(key),
            "feature_up must store dep in dep_records[\"my-feature:notifications-svc\"], got: {:?}",
            state.dep_records.keys().collect::<Vec<_>>());
        assert!(!state.records.contains_key(&0),
            "feature_up must not use pr=0 sentinel for dep slots");
    }

    #[test]
    fn feature_governs_dep_records_use_repo_root_as_worktree() {
        let (ps, _dir) = ps_store();
        let (app_store, _app_dir) = app_store();
        let repo = tempdir().unwrap();
        let mut cfg = echo_config("notifications-svc");
        cfg.main_worktree = None;
        app_store.save_app_config(cfg).unwrap();
        feature_new(&ps, "my-feature").unwrap();
        feature_add_dep(&ps, "my-feature", "notifications-svc").unwrap();
        crate::feature::feature_up(&ps, &app_store, "my-feature", repo.path()).unwrap();
        let state = ps.load().unwrap();
        let rec = &state.dep_records["my-feature:notifications-svc"];
        assert_eq!(rec.worktree, repo.path().to_string_lossy().as_ref(),
            "dep_record worktree must equal repo_root, not cfg.main_worktree");
    }

    #[test]
    fn feature_governs_multiple_deps_each_get_own_dep_record() {
        let (ps, _dir) = ps_store();
        let (app_store, _app_dir) = app_store();
        let repo = tempdir().unwrap();
        app_store.save_app_config(echo_config("backend")).unwrap();
        app_store.save_app_config(echo_config("staff")).unwrap();
        app_store.save_app_config(echo_config("mock-portal")).unwrap();
        feature_new(&ps, "b-form").unwrap();
        feature_add_dep(&ps, "b-form", "backend").unwrap();
        feature_add_dep(&ps, "b-form", "staff").unwrap();
        feature_add_dep(&ps, "b-form", "mock-portal").unwrap();
        crate::feature::feature_up(&ps, &app_store, "b-form", repo.path()).unwrap();
        let state = ps.load().unwrap();
        for dep in &["backend", "staff", "mock-portal"] {
            let key = format!("b-form:{}", dep);
            assert!(state.dep_records.contains_key(&key),
                "dep_records must contain key '{}', got: {:?}", key,
                state.dep_records.keys().collect::<Vec<_>>());
        }
    }

    // feature_add with --config appends to app_config_names on the ProcessRecord
    #[test]
    fn feature_governs_add_with_configs_stores_app_config_names() {
        let (ps, _dir) = ps_store();
        let store = {
            let d = tempdir().unwrap();
            Store::open(d.path())
        };
        // store is dropped immediately but track is called inside feature_add
        let (ps2, _d2) = ps_store();
        let store2 = {
            let d = tempdir().unwrap();
            crate::store::Store::open(d.path())
        };
        feature_add(&ps2, &store2, "my-feature", 7, &["api".to_string(), "worker".to_string()]).unwrap();
        let state = ps2.load().unwrap();
        let rec = &state.records[&7];
        assert_eq!(rec.app_config_names, vec!["api", "worker"],
            "feature_add must store --config values in app_config_names, got: {:?}", rec.app_config_names);
    }

    // bootstrap_pr governs child in new process group (survives terminal SIGHUP)
    #[test]
    #[cfg(unix)]
    fn bootstrap_pr_governs_child_in_new_process_group() {
        let (ps, _dir) = ps_store();
        let wt = tempdir().unwrap();
        let cfg = AppConfig {
            name: "svc".into(),
            bootstrap: "sleep 5".into(),
            teardown: "true".into(),
            startup_timeout: "5s".into(),
            health_check: None,
            ephemeral: false,
            main_worktree: None, setup: None,
        };
        bootstrap_pr(&ps, &cfg, 1, wt.path(), "org", "repo").unwrap();
        let state = ps.load().unwrap();
        let pid = state.records[&1].pid.unwrap();
        let child_pgid = unsafe { libc::getpgid(pid as libc::pid_t) };
        let our_pgid = unsafe { libc::getpgrp() };
        unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
        assert_ne!(child_pgid, our_pgid,
            "bootstrap_pr child must be in its own process group (pgid {}) not fp's (pgid {})",
            child_pgid, our_pgid);
    }

    #[test]
    fn feature_up_governs_derives_worktree_from_repo_root_not_rec_worktree() {
        use std::process::Command;
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        Command::new("git").args(["init", "-b", "main", repo.to_str().unwrap()]).output().unwrap();
        for arg in &[["config","user.email","t@t.com"],["config","user.name","T"]] {
            Command::new("git").args(["-C", repo.to_str().unwrap()]).args(arg).output().unwrap();
        }
        std::fs::write(repo.join("f"), "x").unwrap();
        Command::new("git").args(["-C", repo.to_str().unwrap(), "add", "."]).output().unwrap();
        Command::new("git").args(["-C", repo.to_str().unwrap(), "commit", "--allow-empty", "-m", "init"]).output().unwrap();
        Command::new("git").args(["-C", repo.to_str().unwrap(), "checkout", "-b", "feat/up"]).output().unwrap();
        Command::new("git").args(["-C", repo.to_str().unwrap(), "checkout", "main"]).output().unwrap();
        let wt_dir = tmp.path().join("repo-worktrees").join("feat").join("up");
        std::fs::create_dir_all(wt_dir.parent().unwrap()).unwrap();
        Command::new("git").args(["-C", repo.to_str().unwrap(), "worktree", "add", wt_dir.to_str().unwrap(), "feat/up"]).output().unwrap();
        let git_dir = repo.join(".git");
        let (ps, _ps_dir) = ps_store();
        let (app_cfg_store, cfg_dir) = app_store();
        let cfg = AppConfig {
            name: "svc".into(),
            bootstrap: "true".into(),
            teardown: "true".into(),
            startup_timeout: "1s".into(),
            health_check: None,
            ephemeral: false,
            main_worktree: None, setup: None,
        };
        app_cfg_store.save_app_config(cfg.clone()).unwrap();
        feature_new(&ps, "feat").unwrap();
        let mut state = ps.load().unwrap();
        state.records.insert(42, crate::process_store::ProcessRecord {
            pr: 42,
            expected_branch: "feat/up".into(),
            pid: None,
            feature_envelopes: vec!["feat".into()],
            feature_envelope: None,
            worktree: String::new(), // empty — must NOT be used
            app_config_names: vec!["svc".into()],
        });
        ps.save_state(state).unwrap();
        let result = crate::feature::feature_up(&ps, &app_cfg_store, "feat", &repo);
        assert!(result.is_ok(), "feature_up with repo_root must succeed when worktree derived from branch: {:?}", result);
        let msgs = result.unwrap();
        assert!(msgs.iter().any(|m| m.contains("started")),
            "feature_up must report started when derived worktree exists: {:?}", msgs);
        let _ = git_dir; let _ = cfg_dir;
    }

    #[test]
    fn feature_down_governs_derives_worktree_from_repo_root_not_rec_worktree() {
        use std::process::Command;
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        Command::new("git").args(["init", "-b", "main", repo.to_str().unwrap()]).output().unwrap();
        for arg in &[["config","user.email","t@t.com"],["config","user.name","T"]] {
            Command::new("git").args(["-C", repo.to_str().unwrap()]).args(arg).output().unwrap();
        }
        std::fs::write(repo.join("f"), "x").unwrap();
        Command::new("git").args(["-C", repo.to_str().unwrap(), "add", "."]).output().unwrap();
        Command::new("git").args(["-C", repo.to_str().unwrap(), "commit", "--allow-empty", "-m", "init"]).output().unwrap();
        Command::new("git").args(["-C", repo.to_str().unwrap(), "checkout", "-b", "feat/down"]).output().unwrap();
        Command::new("git").args(["-C", repo.to_str().unwrap(), "checkout", "main"]).output().unwrap();
        let wt_dir = tmp.path().join("repo-worktrees").join("feat").join("down");
        std::fs::create_dir_all(wt_dir.parent().unwrap()).unwrap();
        Command::new("git").args(["-C", repo.to_str().unwrap(), "worktree", "add", wt_dir.to_str().unwrap(), "feat/down"]).output().unwrap();
        let (ps, _) = ps_store();
        let (app_cfg_store, _cfg_dir) = app_store();
        let cfg = AppConfig {
            name: "svc".into(),
            bootstrap: "true".into(),
            teardown: "touch .torn_down".into(), // writes marker to CWD — must be derived worktree
            startup_timeout: "1s".into(),
            health_check: None,
            ephemeral: false,
            main_worktree: None, setup: None,
        };
        app_cfg_store.save_app_config(cfg.clone()).unwrap();
        feature_new(&ps, "feat").unwrap();
        let mut state = ps.load().unwrap();
        state.records.insert(43, crate::process_store::ProcessRecord {
            pr: 43,
            expected_branch: "feat/down".into(),
            pid: None,
            feature_envelopes: vec!["feat".into()],
            feature_envelope: None,
            worktree: String::new(), // empty — must NOT be used
            app_config_names: vec!["svc".into()],
        });
        ps.save_state(state).unwrap();
        let result = crate::feature::feature_down(&ps, &app_cfg_store, "feat", &repo);
        assert!(result.is_ok(), "feature_down with repo_root must succeed when worktree derived from branch: {:?}", result);
        let msgs = result.unwrap();
        assert!(msgs.iter().any(|m| m.contains("stopped")),
            "feature_down must report stopped when derived worktree exists: {:?}", msgs);
        assert!(wt_dir.join(".torn_down").exists(),
            "teardown marker must exist in derived worktree, not in CWD: wt_dir={:?}", wt_dir);
    }

    #[test]
    fn feature_status_governs_no_fallback_to_rec_worktree() {
        use std::process::Command;
        // Set up: repo with two branches; correct worktree at derived path, wrong worktree at rec.worktree
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        Command::new("git").args(["init", "-b", "main", repo.to_str().unwrap()]).output().unwrap();
        for arg in &[["config","user.email","t@t.com"],["config","user.name","T"]] {
            Command::new("git").args(["-C", repo.to_str().unwrap()]).args(arg).output().unwrap();
        }
        std::fs::write(repo.join("f"), "x").unwrap();
        Command::new("git").args(["-C", repo.to_str().unwrap(), "add", "."]).output().unwrap();
        Command::new("git").args(["-C", repo.to_str().unwrap(), "commit", "--allow-empty", "-m", "init"]).output().unwrap();
        // Create feat/status and feat/wrong branches
        for b in &["feat/status", "feat/wrong"] {
            Command::new("git").args(["-C", repo.to_str().unwrap(), "checkout", "-b", b]).output().unwrap();
            Command::new("git").args(["-C", repo.to_str().unwrap(), "checkout", "main"]).output().unwrap();
        }
        // Create correct worktree at derived path
        let correct_wt = crate::worktree::worktree_path(&repo, "feat/status");
        std::fs::create_dir_all(correct_wt.parent().unwrap()).unwrap();
        Command::new("git").args(["-C", repo.to_str().unwrap(), "worktree", "add", correct_wt.to_str().unwrap(), "feat/status"]).output().unwrap();
        // Create wrong-branch worktree at a separate path (simulates old rec.worktree)
        let wrong_wt = crate::worktree::worktree_path(&repo, "feat/wrong");
        std::fs::create_dir_all(wrong_wt.parent().unwrap()).unwrap();
        Command::new("git").args(["-C", repo.to_str().unwrap(), "worktree", "add", wrong_wt.to_str().unwrap(), "feat/wrong"]).output().unwrap();
        let (ps, _) = ps_store();
        let (app_cfg_store, _) = app_store();
        feature_new(&ps, "feat").unwrap();
        let mut state = ps.load().unwrap();
        state.records.insert(44, crate::process_store::ProcessRecord {
            pr: 44,
            expected_branch: "feat/status".into(),
            pid: None,
            feature_envelopes: vec!["feat".into()],
            feature_envelope: None,
            worktree: wrong_wt.to_str().unwrap().to_string(), // must NOT be used — wrong branch
            app_config_names: vec![],
        });
        ps.save_state(state).unwrap();
        // Case 1: derived path exists — must use it (Some(true))
        let result = crate::feature::feature_status(&ps, &app_cfg_store, "feat", &repo);
        assert!(result.is_ok(), "feature_status must succeed: {:?}", result);
        let statuses = result.unwrap();
        assert_eq!(statuses[0].branch_ok, Some(true),
            "branch_ok must be Some(true) using derived path (feat/status worktree): {:?}", statuses[0].branch_ok);

        // Case 2: derived path absent, rec.worktree has wrong-branch worktree — must NOT fall back
        // Use a repo_root that produces a non-existent derived path
        let other_root = tmp.path().join("other_repo");
        let result2 = crate::feature::feature_status(&ps, &app_cfg_store, "feat", &other_root);
        assert!(result2.is_ok(), "feature_status must not error when derived path absent: {:?}", result2);
        let statuses2 = result2.unwrap();
        assert_eq!(statuses2[0].branch_ok, None,
            "branch_ok must be None (not Some(false) from rec.worktree) when derived path absent: {:?}", statuses2[0].branch_ok);
    }

    #[test]
    fn feature_list_running_with_config_governs_derives_worktree_from_repo_root() {
        let (ps, _) = ps_store();
        let (app_cfg_store, _cfg_dir) = app_store();
        let cfg = AppConfig {
            name: "ephem".into(),
            bootstrap: "true".into(),
            teardown: "true".into(),
            startup_timeout: "1s".into(),
            health_check: Some("true".into()),
            ephemeral: true,
            main_worktree: None, setup: None,
        };
        app_cfg_store.save_app_config(cfg.clone()).unwrap();
        feature_new(&ps, "feat").unwrap();
        let tmp = tempfile::tempdir().unwrap();
        // rec.worktree points to a real existing dir so stub (ignoring repo_root) would see health check pass
        let real_existing_dir = tmp.path().to_string_lossy().to_string();
        let mut state = ps.load().unwrap();
        state.records.insert(45, crate::process_store::ProcessRecord {
            pr: 45,
            expected_branch: "feat/ephem".into(),
            pid: None,
            feature_envelopes: vec!["feat".into()],
            feature_envelope: None,
            worktree: real_existing_dir, // real dir — must NOT be used; derived path must be used instead
            app_config_names: vec!["ephem".into()],
        });
        ps.save_state(state).unwrap();
        // Ephemeral apps always appear running (no health check or PID required)
        let result = crate::feature::feature_list_running_with_config(&ps, &app_cfg_store, tmp.path());
        assert!(result.is_ok(), "feature_list_running_with_config must not error: {:?}", result);
        let running = result.unwrap();
        assert!(running.iter().any(|f| f.name == "feat"),
            "ephemeral feature must always appear running regardless of worktree: {:?}", running);
    }

    #[test]
    fn bootstrap_pr_governs_preserves_feature_envelope_and_app_config_names_on_existing_record() {
        let (ps, _dir) = ps_store();
        let wt = tempfile::tempdir().unwrap();
        // Pre-populate record with feature_envelope and app_config_names
        let existing = crate::process_store::ProcessRecord {
            pr: 55,
            expected_branch: "feat/x".into(),
            pid: None,
            feature_envelopes: vec!["my-feat".into()], feature_envelope: None,
            worktree: wt.path().to_string_lossy().to_string(),
            app_config_names: vec!["svc".into()],
        };
        ps.activate(existing).unwrap();
        let cfg = crate::app_config::AppConfig {
            name: "svc".into(),
            bootstrap: "true".into(),
            teardown: "true".into(),
            startup_timeout: "1s".into(),
            health_check: None,
            ephemeral: false,
            main_worktree: None, setup: None,
        };
        bootstrap_pr(&ps, &cfg, 55, wt.path(), "org", "repo").unwrap();
        let state = ps.load().unwrap();
        let rec = &state.records[&55];
        assert!(rec.in_envelope("my-feat"),
            "bootstrap_pr must not overwrite feature envelope on existing record");
        assert_eq!(rec.app_config_names, vec!["svc"],
            "bootstrap_pr must not overwrite app_config_names on existing record, got: {:?}", rec.app_config_names);
        assert_eq!(rec.expected_branch, "feat/x",
            "bootstrap_pr must not overwrite expected_branch on existing record, got: {:?}", rec.expected_branch);
        assert!(rec.pid.is_some(), "bootstrap_pr must set pid on existing record");
        if let Some(pid) = rec.pid { unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM); } }
    }

    #[test]
    fn resolve_worktree_governs_empty_branch_returns_repo_root() {
        let root = std::path::Path::new("/some/repo");
        let result = resolve_worktree(root, "");
        assert_eq!(result, std::path::Path::new("/some/repo"),
            "resolve_worktree with empty branch must return repo_root, got: {:?}", result);
    }

    #[test]
    fn resolve_worktree_governs_nonempty_branch_returns_worktree_path() {
        let root = std::path::Path::new("/some/repo");
        let result = resolve_worktree(root, "feat/x");
        let expected = crate::worktree::worktree_path(root, "feat/x");
        assert_eq!(result, expected,
            "resolve_worktree with non-empty branch must return worktree_path, got: {:?}", result);
    }

    #[test]
    fn feature_governs_down_tears_down_dep_records_for_envelope() {
        let (ps, _dir) = ps_store();
        let (app_store, _app_dir) = app_store();
        let repo = tempdir().unwrap();
        let mut cfg = echo_config("backend");
        cfg.main_worktree = None;
        app_store.save_app_config(cfg).unwrap();
        feature_new(&ps, "my-feature").unwrap();
        feature_add_dep(&ps, "my-feature", "backend").unwrap();
        crate::feature::feature_up(&ps, &app_store, "my-feature", repo.path()).unwrap();
        // confirm dep_record exists before down
        let state = ps.load().unwrap();
        assert!(state.dep_records.contains_key("my-feature:backend"),
            "pre-condition: dep_records must have my-feature:backend before down");
        crate::feature::feature_down(&ps, &app_store, "my-feature", repo.path()).unwrap();
        let state_after = ps.load().unwrap();
        assert!(!state_after.dep_records.contains_key("my-feature:backend"),
            "feature_down must remove my-feature:backend from dep_records, got: {:?}",
            state_after.dep_records.keys().collect::<Vec<_>>());
    }

    #[test]
    fn feature_governs_rebuild_pr0_reruns_dep_bootstrap_and_updates_pid() {
        let (ps, _dir) = ps_store();
        let (app_store, _app_dir) = app_store();
        let repo = tempdir().unwrap();
        app_store.save_app_config(echo_config("backend")).unwrap();
        feature_new(&ps, "my-feature").unwrap();
        feature_add_dep(&ps, "my-feature", "backend").unwrap();
        crate::feature::feature_up(&ps, &app_store, "my-feature", repo.path()).unwrap();
        let pid_before = ps.load().unwrap().dep_records["my-feature:backend"].pid;
        crate::feature::feature_rebuild(&ps, &app_store, "my-feature", Some(0), repo.path()).unwrap();
        let pid_after = ps.load().unwrap().dep_records["my-feature:backend"].pid;
        assert!(pid_after.is_some(), "rebuild must set a pid in dep_records");
        assert_ne!(pid_before, pid_after,
            "rebuild must update dep_records pid to new bootstrap process");
    }

    #[test]
    fn feature_status_governs_dep_slot_uses_repo_root_as_worktree() {
        let (ps, _) = ps_store();
        let (app_cfg_store, _) = app_store();
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        app_cfg_store.save_app_config(ephemeral_config("svc")).unwrap();
        feature_new(&ps, "feat").unwrap();
        let mut state = ps.load().unwrap();
        // dep slot: pr=0, expected_branch=""
        state.records.insert(0, crate::process_store::ProcessRecord {
            pr: 0,
            expected_branch: String::new(),
            pid: None,
            feature_envelopes: vec!["feat".into()],
            feature_envelope: None,
            worktree: String::new(),
            app_config_names: vec!["svc".into()],
        });
        ps.save_state(state).unwrap();
        // feature_status must not error for dep slot — it should use repo_root as worktree
        let result = crate::feature::feature_status(&ps, &app_cfg_store, "feat", &repo);
        assert!(result.is_ok(),
            "feature_status must not error for dep slot with empty expected_branch: {:?}", result);
        let statuses = result.unwrap();
        assert_eq!(statuses.len(), 1,
            "feature_status must return one entry for dep slot pr=0: {:?}", statuses);
    }

    #[test]
    fn feature_list_running_with_config_governs_dep_slot_uses_repo_root() {
        let (ps, _) = ps_store();
        let (app_cfg_store, _) = app_store();
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        // ephemeral config with health_check that succeeds when run from repo root
        let cfg = crate::app_config::AppConfig {
            name: "svc".into(),
            bootstrap: "true".into(),
            teardown: "true".into(),
            startup_timeout: "1s".into(),
            health_check: Some("true".into()),
            ephemeral: true,
            main_worktree: None, setup: None,
        };
        app_cfg_store.save_app_config(cfg).unwrap();
        feature_new(&ps, "feat").unwrap();
        let mut state = ps.load().unwrap();
        state.records.insert(0, crate::process_store::ProcessRecord {
            pr: 0,
            expected_branch: String::new(),
            pid: None,
            feature_envelopes: vec!["feat".into()],
            feature_envelope: None,
            worktree: String::new(),
            app_config_names: vec!["svc".into()],
        });
        ps.save_state(state).unwrap();
        // repo exists as a directory — health check "true" should pass from repo_root
        let result = crate::feature::feature_list_running_with_config(&ps, &app_cfg_store, &repo);
        assert!(result.is_ok(), "feature_list_running_with_config must not error for dep slot: {:?}", result);
        let running = result.unwrap();
        assert!(running.iter().any(|f| f.name == "feat"),
            "dep slot with passing health_check must appear running when repo_root exists: {:?}", running);
    }

    #[test]
    fn bootstrap_pr_governs_spawned_process_in_new_session() {
        // Spawned process must be in a different session from the calling process
        // so it survives terminal close (SIGHUP) and fp process exit
        let (ps, _dir) = ps_store();
        let wt = tempdir().unwrap();
        let cfg = AppConfig {
            name: "svc".into(), bootstrap: "sleep 30".into(), teardown: "true".into(),
            startup_timeout: "1s".into(), health_check: None, ephemeral: false, main_worktree: None, setup: None,
        };
        bootstrap_pr(&ps, &cfg, 55, wt.path(), "org", "repo").unwrap();
        let state = ps.load().unwrap();
        let pid = state.records[&55].pid.unwrap() as libc::pid_t;
        let child_sid = unsafe { libc::getsid(pid) };
        let our_sid = unsafe { libc::getsid(0) };
        unsafe { libc::kill(pid, libc::SIGTERM); }
        assert_ne!(child_sid, our_sid,
            "bootstrap_pr must spawn in a new session (setsid) so child survives terminal close");
    }

    #[test]
    fn teardown_pr_governs_preserves_record_with_feature_envelope() {
        let (ps, _dir) = ps_store();
        let wt = tempdir().unwrap();
        let cfg = AppConfig {
            name: "svc".into(), bootstrap: "true".into(), teardown: "true".into(),
            startup_timeout: "1s".into(), health_check: None, ephemeral: false, main_worktree: None, setup: None,
        };
        ps.activate(ProcessRecord {
            pr: 88, expected_branch: "feat/x".into(), pid: Some(99999),
            feature_envelopes: vec!["my-feat".into()], feature_envelope: None,
            worktree: wt.path().to_string_lossy().to_string(),
            app_config_names: vec!["svc".into()],
        }).unwrap();
        teardown_pr(&ps, &cfg, 88, wt.path(), "", "").unwrap();
        let state = ps.load().unwrap();
        assert!(state.records.contains_key(&88),
            "teardown_pr must preserve record (keep feature_envelope) after teardown");
        let rec = &state.records[&88];
        assert!(rec.pid.is_none(), "teardown_pr must clear pid after teardown");
        assert!(rec.in_envelope("my-feat"),
            "teardown_pr must preserve feature envelope after teardown");
    }

    #[test]
    fn feature_down_up_cycle_governs_record_visible_in_feature_status() {
        use std::process::Command;
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        Command::new("git").args(["init", "-b", "main", repo.to_str().unwrap()]).output().unwrap();
        for arg in &[["config","user.email","t@t.com"],["config","user.name","T"]] {
            Command::new("git").args(["-C", repo.to_str().unwrap()]).args(arg).output().unwrap();
        }
        std::fs::write(repo.join("f"), "x").unwrap();
        Command::new("git").args(["-C", repo.to_str().unwrap(), "add", "."]).output().unwrap();
        Command::new("git").args(["-C", repo.to_str().unwrap(), "commit", "--allow-empty", "-m", "init"]).output().unwrap();
        Command::new("git").args(["-C", repo.to_str().unwrap(), "checkout", "-b", "feat/x"]).output().unwrap();
        Command::new("git").args(["-C", repo.to_str().unwrap(), "checkout", "main"]).output().unwrap();
        let wt = crate::worktree::worktree_path(&repo, "feat/x");
        std::fs::create_dir_all(wt.parent().unwrap()).unwrap();
        Command::new("git").args(["-C", repo.to_str().unwrap(), "worktree", "add", wt.to_str().unwrap(), "feat/x"]).output().unwrap();
        let (ps, _dir) = ps_store();
        let (app_store, _app_dir) = app_store();
        let cfg = AppConfig {
            name: "svc".into(), bootstrap: "true".into(), teardown: "true".into(),
            startup_timeout: "1s".into(), health_check: None, ephemeral: false, main_worktree: None, setup: None,
        };
        app_store.save_app_config(cfg).unwrap();
        feature_new(&ps, "my-feat").unwrap();
        ps.activate(ProcessRecord {
            pr: 78280, expected_branch: "feat/x".into(), pid: None,
            feature_envelopes: vec!["my-feat".into()], feature_envelope: None,
            worktree: String::new(),
            app_config_names: vec!["svc".into()],
        }).unwrap();
        crate::feature::feature_down(&ps, &app_store, "my-feat", &repo).unwrap();
        crate::feature::feature_up(&ps, &app_store, "my-feat", &repo).unwrap();
        let statuses = feature_status(&ps, &app_store, "my-feat", &repo).unwrap();
        assert!(statuses.iter().any(|s| s.pr == 78280),
            "PR 78280 must appear in feature_status after down→up cycle, got: {:?}",
            statuses.iter().map(|s| s.pr).collect::<Vec<_>>());
    }

    #[test]
    fn cmd_feature_status_governs_shows_dep_records_in_text_output() {
        let dir = tempfile::tempdir().unwrap();
        let ps = crate::process_store::ProcessStateStore::open(dir.path());
        let app_store = crate::app_config::AppConfigStore::open(dir.path().join("config.toml"));
        app_store.save_app_config(AppConfig {
            name: "backend".into(), bootstrap: "true".into(), teardown: "true".into(),
            startup_timeout: "1s".into(), health_check: Some("true".into()), ephemeral: true,
            main_worktree: None, setup: None,
        }).unwrap();
        feature_new(&ps, "my-feat").unwrap();
        let mut state = ps.load().unwrap();
        state.dep_records.insert("my-feat:backend".into(), crate::process_store::DepRecord {
            app_config_name: "backend".into(),
            feature_envelope: "my-feat".into(),
            pid: None,
            worktree: dir.path().to_string_lossy().to_string(),
        });
        ps.save_state(state).unwrap();
        let out = crate::commands::cmd_feature_status(&ps, &app_store, "my-feat", false, dir.path()).unwrap();
        assert!(out.contains("backend"),
            "feature status must include dep slot 'backend' in output, got: {}", out);
    }

    #[test]
    fn feature_up_governs_errors_when_service_healthy_but_pid_dead() {
        let tmp = tempfile::tempdir().unwrap();
        let (ps, _ps_dir) = ps_store();
        let (app_cfg_store, _cfg_dir) = app_store();
        let cfg = AppConfig {
            name: "svc".into(),
            bootstrap: "true".into(),
            teardown: "true".into(),
            startup_timeout: "1s".into(),
            health_check: Some("true".into()),
            ephemeral: false,
            main_worktree: None, setup: None,
        };
        app_cfg_store.save_app_config(cfg).unwrap();
        crate::feature::feature_new(&ps, "my-feat").unwrap();
        let mut state = ps.load().unwrap();
        state.records.insert(99, crate::process_store::ProcessRecord {
            pr: 99,
            expected_branch: "".into(), // empty so resolve_worktree returns repo_root (tmp.path())
            pid: None,
            feature_envelopes: vec!["my-feat".into()], feature_envelope: None,
            worktree: tmp.path().to_string_lossy().to_string(),
            app_config_names: vec!["svc".into()],
        });
        ps.save_state(state).unwrap();
        let result = crate::feature::feature_up(&ps, &app_cfg_store, "my-feat", tmp.path());
        assert!(result.is_err(), "feature_up must error when service is healthy but pid is dead (untracked process)");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("healthy but untracked"),
            "feature_up error must mention 'healthy but untracked', got: {}", err);
    }

    // D_multi1: feature_add keeps PR in first envelope when added to a second
    #[test]
    fn feature_governs_add_pr_stays_in_first_envelope_after_add_to_second() {
        let (ps, _dir) = ps_store();
        let git_dir = tempdir().unwrap();
        let store = git_store(git_dir.path());
        store.track(42).unwrap();
        store.update_cache(PrCache { number: 42, title: "T".into(), branch: "feat/x".into(), base: "main".into() }).unwrap();
        feature_new(&ps, "envelope-a").unwrap();
        feature_new(&ps, "envelope-b").unwrap();
        feature_add(&ps, &store, "envelope-a", 42, &[]).unwrap();
        feature_add(&ps, &store, "envelope-b", 42, &[]).unwrap();
        let list = feature_list(&ps).unwrap();
        let a = list.iter().find(|f| f.name == "envelope-a").unwrap();
        assert!(a.prs.contains(&42),
            "PR 42 must remain in envelope-a after being added to envelope-b, got: {:?}", a.prs);
    }

    // D_multi2: feature_add PR appears in both envelopes simultaneously
    #[test]
    fn feature_governs_add_pr_appears_in_both_envelopes() {
        let (ps, _dir) = ps_store();
        let git_dir = tempdir().unwrap();
        let store = git_store(git_dir.path());
        store.track(42).unwrap();
        store.update_cache(PrCache { number: 42, title: "T".into(), branch: "feat/x".into(), base: "main".into() }).unwrap();
        feature_new(&ps, "envelope-a").unwrap();
        feature_new(&ps, "envelope-b").unwrap();
        feature_add(&ps, &store, "envelope-a", 42, &[]).unwrap();
        feature_add(&ps, &store, "envelope-b", 42, &[]).unwrap();
        let list = feature_list(&ps).unwrap();
        let a = list.iter().find(|f| f.name == "envelope-a").unwrap();
        let b = list.iter().find(|f| f.name == "envelope-b").unwrap();
        assert!(a.prs.contains(&42) && b.prs.contains(&42),
            "PR 42 must appear in both envelope-a and envelope-b, got a.prs={:?}, b.prs={:?}", a.prs, b.prs);
    }
}
