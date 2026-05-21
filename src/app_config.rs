use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub name: String,
    pub bootstrap: String,
    pub teardown: String,
    pub startup_timeout: String,
    pub health_check: Option<String>,
    #[serde(default)]
    pub ephemeral: bool,
    #[serde(default)]
    pub main_worktree: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct AppConfigState {
    #[serde(default)]
    configs: HashMap<String, AppConfig>,
    #[serde(default)]
    repo_assignments: HashMap<String, String>,
}

pub struct AppConfigStore {
    path: PathBuf,
}

impl AppConfigStore {
    pub fn open(path: PathBuf) -> Self {
        AppConfigStore { path }
    }

    pub fn default_path() -> Result<PathBuf> {
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("could not determine home directory"))?;
        Ok(home.join(".fp").join("config.toml"))
    }

    fn load_state(&self) -> Result<AppConfigState> {
        if !self.path.exists() {
            return Ok(AppConfigState::default());
        }
        let toml_str = fs::read_to_string(&self.path)?;
        let state = toml::from_str(&toml_str)?;
        Ok(state)
    }

    fn save_state(&self, state: &AppConfigState) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let toml_str = toml::to_string_pretty(state)?;
        fs::write(&self.path, toml_str)?;
        Ok(())
    }

    pub fn save_app_config(&self, config: AppConfig) -> Result<()> {
        let mut state = self.load_state()?;
        state.configs.insert(config.name.clone(), config);
        self.save_state(&state)
    }

    pub fn load_app_config(&self, name: &str) -> Result<Option<AppConfig>> {
        let state = self.load_state()?;
        Ok(state.configs.get(name).cloned())
    }

    pub fn set_repo_config(&self, repo: &str, config_name: &str) -> Result<()> {
        let mut state = self.load_state()?;
        state.repo_assignments.insert(repo.to_string(), config_name.to_string());
        self.save_state(&state)
    }

    pub fn get_repo_config(&self, repo: &str) -> Result<Option<String>> {
        let state = self.load_state()?;
        Ok(state.repo_assignments.get(repo).cloned())
    }

    pub fn list_app_configs(&self) -> Result<Vec<AppConfig>> {
        let state = self.load_state()?;
        let mut configs: Vec<AppConfig> = state.configs.into_values().collect();
        configs.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(configs)
    }

}
