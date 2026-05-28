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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) feature_envelope: Option<String>,
    #[serde(default)]
    pub feature_envelopes: Vec<String>,
    pub worktree: String,
    #[serde(default)]
    pub app_config_names: Vec<String>,
}

impl ProcessRecord {
    pub fn new(pr: u64, expected_branch: String, worktree: String) -> Self {
        ProcessRecord {
            pr,
            expected_branch,
            pid: None,
            feature_envelope: None,
            feature_envelopes: vec![],
            worktree,
            app_config_names: vec![],
        }
    }

    pub fn in_envelope(&self, name: &str) -> bool {
        self.feature_envelopes.contains(&name.to_string())
            || self.feature_envelope.as_deref() == Some(name)
    }

    pub fn add_envelope(&mut self, name: &str) {
        let s = name.to_string();
        if !self.feature_envelopes.contains(&s) {
            self.feature_envelopes.push(s);
        }
        self.feature_envelope = None;
    }

    pub fn remove_envelope(&mut self, name: &str) {
        self.feature_envelopes.retain(|e| e != name);
        if self.feature_envelope.as_deref() == Some(name) {
            self.feature_envelope = None;
        }
    }

    pub fn envelopes(&self) -> Vec<&str> {
        if !self.feature_envelopes.is_empty() {
            self.feature_envelopes.iter().map(|s| s.as_str()).collect()
        } else if let Some(e) = &self.feature_envelope {
            vec![e.as_str()]
        } else {
            vec![]
        }
    }
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
    #[serde(default)]
    pub setup_completed: HashSet<(String, String)>,
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
