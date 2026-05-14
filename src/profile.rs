use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub github_token: String,
    pub repo: String,
}

pub fn save_profile(path: &Path, name: &str, github_token: &str, repo: &str) -> Result<()> {
    let mut profiles: HashMap<String, Profile> = if path.exists() {
        serde_json::from_str(&std::fs::read_to_string(path)?)?
    } else {
        HashMap::new()
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    profiles.insert(name.to_string(), Profile {
        github_token: github_token.to_string(),
        repo: repo.to_string(),
    });
    std::fs::write(path, serde_json::to_string_pretty(&profiles)?)?;
    Ok(())
}

pub fn load_profile(path: &Path, name: &str) -> Result<Profile> {
    if !path.exists() {
        anyhow::bail!("profile not found: {}", name);
    }
    let profiles: HashMap<String, Profile> = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    profiles.into_iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v)
        .ok_or_else(|| anyhow::anyhow!("profile not found: {}", name))
}
