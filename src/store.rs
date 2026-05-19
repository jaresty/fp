// Local state store — .git/fp/state.json

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Cached data about a tracked PR, refreshed from the GitHub API on each fetch.
/// This is NOT the authoritative source — always prefer live API data.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PrCache {
    pub number: u64,
    pub title: String,
    pub branch: String,
    pub base: String,
}

/// For migration: deserialize old-format state.json which stored TrackedPr in `prs`.
#[derive(Deserialize, Default)]
struct LegacyState {
    #[serde(default)]
    prs: HashMap<u64, PrCache>,
    #[serde(default)]
    cached_merge_methods: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct State {
    /// The set of PR numbers the user has asked fp to track.
    #[serde(default)]
    pub tracked: HashSet<u64>,
    /// Cached PR metadata, populated and replaced on each API fetch.
    #[serde(default)]
    pub cache: HashMap<u64, PrCache>,
    #[serde(default)]
    pub cached_merge_methods: HashMap<String, String>,
}

impl State {
    /// Returns the cached PR entries for all tracked PRs that have cache data.
    pub fn tracked_prs(&self) -> Vec<&PrCache> {
        let mut prs: Vec<&PrCache> = self.tracked.iter()
            .filter_map(|n| self.cache.get(n))
            .collect();
        prs.sort_by_key(|p| p.number);
        prs
    }
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
        // Try new format first
        if let Ok(state) = serde_json::from_str::<State>(&s)
            && !state.tracked.is_empty() {
            return Ok(state);
        }
        // Migrate from old format (had `prs` key with TrackedPr entries)
        if let Ok(legacy) = serde_json::from_str::<LegacyState>(&s)
            && !legacy.prs.is_empty() {
            let tracked: HashSet<u64> = legacy.prs.keys().copied().collect();
            let cache: HashMap<u64, PrCache> = legacy.prs;
            return Ok(State { tracked, cache, cached_merge_methods: legacy.cached_merge_methods });
        }
        Ok(serde_json::from_str(&s)?)
    }

    pub fn save(&self, state: &State) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.path, serde_json::to_string_pretty(state)?)?;
        Ok(())
    }

    /// Add a PR number to the tracked set.
    pub fn track(&self, number: u64) -> Result<()> {
        let mut state = self.load()?;
        state.tracked.insert(number);
        self.save(&state)
    }

    /// Store or replace the cached API data for a PR.
    pub fn update_cache(&self, pr: PrCache) -> Result<()> {
        let mut state = self.load()?;
        state.cache.insert(pr.number, pr);
        self.save(&state)
    }

    /// Replace the entire cache with fresh API data (wipes stale entries).
    pub fn replace_cache(&self, cache: HashMap<u64, PrCache>) -> Result<()> {
        let mut state = self.load()?;
        state.cache = cache;
        self.save(&state)
    }

    pub fn untrack(&self, number: u64) -> Result<()> {
        let mut state = self.load()?;
        state.tracked.remove(&number);
        state.cache.remove(&number);
        self.save(&state)
    }
}
