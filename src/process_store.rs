use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepRecord {
    pub app_config_name: String,
    pub feature_envelope: String,
    pub pid: Option<u32>,
    pub worktree: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessRecord {
    pub pr: u64,
    pub expected_branch: String,
    pub pid: Option<u32>,
    pub feature_envelope: Option<String>,
    pub worktree: String,
    #[serde(default)]
    pub app_config_names: Vec<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct FeatureConfig {
    pub test_command: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ProcessState {
    pub records: HashMap<u64, ProcessRecord>,
    #[serde(default)]
    pub feature_envelopes: HashSet<String>,
    #[serde(default)]
    pub envelope_deps: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub dep_records: HashMap<String, DepRecord>,
    #[serde(default)]
    pub feature_configs: HashMap<String, FeatureConfig>,
}

pub struct ProcessStateStore {
    pub(crate) path: PathBuf,
}

impl ProcessStateStore {
    pub fn open(git_dir: &std::path::Path) -> Self {
        ProcessStateStore { path: git_dir.join("fp").join("process-state.json") }
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
