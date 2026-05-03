// Local state store — .git/fp/state.json

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TrackedPr {
    pub number: u64,
    pub title: String,
    pub branch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct State {
    pub prs: HashMap<u64, TrackedPr>,
}

pub struct Store {
    path: PathBuf,
}

impl Store {
    pub fn open(git_dir: &Path) -> Self {
        Store { path: git_dir.join("fp").join("state.json") }
    }

    pub fn load(&self) -> Result<State> {
        if !self.path.exists() {
            return Ok(State::default());
        }
        let s = std::fs::read_to_string(&self.path)?;
        Ok(serde_json::from_str(&s)?)
    }

    pub fn save(&self, state: &State) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.path, serde_json::to_string_pretty(state)?)?;
        Ok(())
    }

    pub fn track(&self, pr: TrackedPr) -> Result<()> {
        let mut state = self.load()?;
        state.prs.insert(pr.number, pr);
        self.save(&state)
    }

    pub fn untrack(&self, number: u64) -> Result<()> {
        let mut state = self.load()?;
        state.prs.remove(&number);
        self.save(&state)
    }
}
