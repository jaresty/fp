use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessRecord {
    pub pr: u64,
    pub expected_branch: String,
    pub pid: Option<u32>,
    pub feature_envelope: Option<String>,
    pub worktree: String,
    #[serde(default)]
    pub app_config_name: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ProcessState {
    pub records: HashMap<u64, ProcessRecord>,
    #[serde(default)]
    pub feature_envelopes: HashSet<String>,
}

pub struct ProcessStateStore {
    path: PathBuf,
}

impl ProcessStateStore {
    pub fn open(path: PathBuf) -> Self {
        ProcessStateStore { path }
    }

    pub fn default_path() -> Result<PathBuf> {
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("could not determine home directory"))?;
        Ok(home.join(".fp").join("process-state.json"))
    }

    pub fn load(&self) -> Result<ProcessState> {
        if !self.path.exists() {
            return Ok(ProcessState::default());
        }
        let json = fs::read_to_string(&self.path)?;
        let state = serde_json::from_str(&json)?;
        Ok(state)
    }

    pub fn save_state(&self, state: ProcessState) -> Result<()> {
        let json = serde_json::to_string_pretty(&state)?;
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.path, json)?;
        Ok(())
    }

    pub fn activate(&self, record: ProcessRecord) -> Result<()> {
        let mut state = self.load()?;
        state.records.insert(record.pr, record);
        let json = serde_json::to_string_pretty(&state)?;
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.path, json)?;
        Ok(())
    }

    pub fn deactivate(&self, pr: u64) -> Result<()> {
        let mut state = self.load()?;
        state.records.remove(&pr);
        let json = serde_json::to_string_pretty(&state)?;
        fs::write(&self.path, json)?;
        Ok(())
    }
}
