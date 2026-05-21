#[cfg(test)]
mod tests {
    use crate::app_config::{AppConfig, AppConfigStore};
    use tempfile::tempdir;

    fn make_store() -> (AppConfigStore, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let store = AppConfigStore::open(dir.path().join("config.toml"));
        (store, dir)
    }

    fn sample_config(name: &str) -> AppConfig {
        AppConfig {
            name: name.into(),
            bootstrap: "docker-compose up -d".into(),
            teardown: "docker-compose down".into(),
            startup_timeout: "60s".into(),
            health_check: None,
            ephemeral: false,
            main_worktree: None,
        }
    }

    #[test]
    fn app_config_store_governs_main_worktree_defaults_none() {
        let (store, _dir) = make_store();
        store.save_app_config(sample_config("svc")).unwrap();
        let cfg = store.load_app_config("svc").unwrap().unwrap();
        assert!(cfg.main_worktree.is_none(),
            "main_worktree must default to None, got: {:?}", cfg.main_worktree);
    }

    #[test]
    fn app_config_store_governs_main_worktree_persists() {
        let (store, _dir) = make_store();
        let mut cfg = sample_config("svc");
        cfg.main_worktree = Some("/repos/svc".into());
        store.save_app_config(cfg).unwrap();
        let loaded = store.load_app_config("svc").unwrap().unwrap();
        assert_eq!(loaded.main_worktree, Some("/repos/svc".into()),
            "main_worktree must persist, got: {:?}", loaded.main_worktree);
    }

    // ephemeral: false is the default when loading a config that omits the field
    #[test]
    fn app_config_store_governs_ephemeral_defaults_false() {
        let (store, _dir) = make_store();
        store.save_app_config(sample_config("svc")).unwrap();
        let cfg = store.load_app_config("svc").unwrap().unwrap();
        assert!(!cfg.ephemeral,
            "ephemeral must default to false, got: {}", cfg.ephemeral);
    }

    // ephemeral: true round-trips through TOML
    #[test]
    fn app_config_store_governs_ephemeral_true_persists() {
        let (store, _dir) = make_store();
        let mut cfg = sample_config("ext");
        cfg.ephemeral = true;
        cfg.health_check = Some("test -f /tmp/ext".into());
        store.save_app_config(cfg).unwrap();
        let loaded = store.load_app_config("ext").unwrap().unwrap();
        assert!(loaded.ephemeral,
            "ephemeral must persist as true, got: {}", loaded.ephemeral);
    }

    // D1: save and load an app config by name
    #[test]
    fn app_config_store_governs_save_and_load_config() {
        let (store, _dir) = make_store();
        store.save_app_config(sample_config("payments-api")).unwrap();
        let loaded = store.load_app_config("payments-api").unwrap();
        assert!(loaded.is_some(),
            "app_config_store::save_app_config must persist config 'payments-api', got None");
        let cfg = loaded.unwrap();
        assert_eq!(cfg.bootstrap, "docker-compose up -d",
            "loaded config must have correct bootstrap, got: {:?}", cfg.bootstrap);
        assert_eq!(cfg.teardown, "docker-compose down",
            "loaded config must have correct teardown, got: {:?}", cfg.teardown);
        assert_eq!(cfg.startup_timeout, "60s",
            "loaded config must have correct startup_timeout, got: {:?}", cfg.startup_timeout);
        assert_eq!(cfg.health_check, None,
            "loaded config must have None health_check, got: {:?}", cfg.health_check);
    }

    // D1b: health_check field round-trips when set
    #[test]
    fn app_config_store_governs_health_check_field_round_trips() {
        let (store, _dir) = make_store();
        let mut cfg = sample_config("svc");
        cfg.health_check = Some("curl -f http://localhost:8080/health".into());
        store.save_app_config(cfg).unwrap();
        let loaded = store.load_app_config("svc").unwrap().unwrap();
        assert_eq!(loaded.health_check, Some("curl -f http://localhost:8080/health".into()),
            "health_check must round-trip, got: {:?}", loaded.health_check);
    }

    // D2: set_repo_config saves repo→config-name mapping; get_repo_config retrieves it
    #[test]
    fn app_config_store_governs_repo_config_assignment() {
        let (store, _dir) = make_store();
        store.set_repo_config("acme/payments-api", "payments-api").unwrap();
        let result = store.get_repo_config("acme/payments-api").unwrap();
        assert_eq!(result, Some("payments-api".to_string()),
            "get_repo_config must return assigned config name, got: {:?}", result);
    }

    // D2b: get_repo_config returns None for unassigned repo
    #[test]
    fn app_config_store_governs_unassigned_repo_returns_none() {
        let (store, _dir) = make_store();
        let result = store.get_repo_config("acme/unassigned").unwrap();
        assert_eq!(result, None,
            "get_repo_config must return None for unassigned repo, got: {:?}", result);
    }


    // D4: default_path returns path ending in .fp/config.toml
    #[test]
    fn app_config_store_governs_default_path_ends_with_fp_config_toml() {
        let path = AppConfigStore::default_path().unwrap();
        let path_str = path.to_string_lossy();
        assert!(path_str.ends_with(".fp/config.toml"),
            "AppConfigStore::default_path must end with .fp/config.toml, got: {}", path_str);
    }

    // multiple configs coexist
    #[test]
    fn app_config_store_governs_multiple_configs_coexist() {
        let (store, _dir) = make_store();
        store.save_app_config(sample_config("svc-a")).unwrap();
        store.save_app_config(sample_config("svc-b")).unwrap();
        assert!(store.load_app_config("svc-a").unwrap().is_some(), "svc-a must be present");
        assert!(store.load_app_config("svc-b").unwrap().is_some(), "svc-b must be present");
    }

    // overwrite: second save replaces first for same config name
    #[test]
    fn app_config_store_governs_save_overwrites_existing_config() {
        let (store, _dir) = make_store();
        store.save_app_config(sample_config("svc")).unwrap();
        let mut updated = sample_config("svc");
        updated.bootstrap = "npm start".into();
        store.save_app_config(updated).unwrap();
        let loaded = store.load_app_config("svc").unwrap().unwrap();
        assert_eq!(loaded.bootstrap, "npm start",
            "second save must overwrite bootstrap, got: {:?}", loaded.bootstrap);
    }
}
