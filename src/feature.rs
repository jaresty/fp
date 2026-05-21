use std::path::Path;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use crate::process_store::{ProcessRecord, ProcessStateStore};
use crate::app_config::AppConfig;
use crate::store::Store;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureInfo {
    pub name: String,
    pub prs: Vec<u64>,
}

#[derive(Debug)]
pub enum ConflictResult {
    NoConflict,
    Conflict { blocking_feature: String, blocking_prs: Vec<u64> },
}

#[derive(Debug, Clone)]
pub struct PrHealthStatus {
    pub pr: u64,
    pub pid_alive: bool,
    pub service_healthy: Option<bool>,
    pub branch_ok: bool,
}

pub fn feature_list_running(ps: &ProcessStateStore) -> Result<Vec<FeatureInfo>> {
    let state = ps.load()?;
    let running: Vec<FeatureInfo> = state.feature_envelopes.iter().filter(|name| {
        state.records.values().any(|r| {
            r.feature_envelope.as_deref() == Some(name) && r.pid.map(health_check_pid).unwrap_or(false)
        })
    }).map(|name| {
        let prs = state.records.values()
            .filter(|r| r.feature_envelope.as_deref() == Some(name))
            .map(|r| r.pr)
            .collect();
        FeatureInfo { name: name.clone(), prs }
    }).collect();
    Ok(running)
}

pub fn feature_list_running_with_config(ps: &ProcessStateStore, config: &crate::app_config::AppConfigStore) -> Result<Vec<FeatureInfo>> {
    let state = ps.load()?;
    let running: Vec<FeatureInfo> = state.feature_envelopes.iter().filter(|name| {
        state.records.values().any(|r| {
            if r.feature_envelope.as_deref() != Some(name) { return false; }
            let app_cfg = r.app_config_name.as_deref()
                .and_then(|n| config.load_app_config(n).ok().flatten());
            let is_ephemeral = app_cfg.as_ref().map(|c| c.ephemeral).unwrap_or(false);
            if is_ephemeral {
                app_cfg.and_then(|c| c.health_check)
                    .map(|cmd| health_check_service(&cmd, std::path::Path::new(&r.worktree), r.pr, std::path::Path::new(&r.worktree)))
                    .unwrap_or(false)
            } else {
                r.pid.map(health_check_pid).unwrap_or(false)
            }
        })
    }).map(|name| {
        let prs = state.records.values()
            .filter(|r| r.feature_envelope.as_deref() == Some(name))
            .map(|r| r.pr)
            .collect();
        FeatureInfo { name: name.clone(), prs }
    }).collect();
    Ok(running)
}

pub fn feature_status(ps: &ProcessStateStore, config: &crate::app_config::AppConfigStore, name: &str) -> Result<Vec<PrHealthStatus>> {
    let state = ps.load()?;
    let mut statuses: Vec<PrHealthStatus> = state.records.values()
        .filter(|r| r.feature_envelope.as_deref() == Some(name))
        .map(|r| {
            let worktree = std::path::Path::new(&r.worktree);
            let branch_ok = health_check_branch(worktree, &r.expected_branch);
            let app_cfg = r.app_config_name.as_deref()
                .and_then(|n| config.load_app_config(n).ok().flatten());
            let is_ephemeral = app_cfg.as_ref().map(|c| c.ephemeral).unwrap_or(false);
            if is_ephemeral {
                let service_healthy = app_cfg.and_then(|c| c.health_check)
                    .map(|cmd| health_check_service(&cmd, worktree, r.pr, worktree));
                PrHealthStatus { pr: r.pr, pid_alive: false, service_healthy, branch_ok }
            } else {
                let pid_alive = r.pid.map(health_check_pid).unwrap_or(false);
                let service_healthy = app_cfg.and_then(|c| c.health_check)
                    .map(|cmd| health_check_service(&cmd, worktree, r.pr, worktree));
                PrHealthStatus { pr: r.pr, pid_alive, service_healthy, branch_ok }
            }
        })
        .collect();
    statuses.sort_by_key(|s| s.pr);
    Ok(statuses)
}

pub fn feature_new(ps: &ProcessStateStore, name: &str) -> Result<()> {
    let mut state = ps.load()?;
    state.feature_envelopes.insert(name.to_string());
    ps.save_state(state)
}

pub fn feature_add(ps: &ProcessStateStore, store: &Store, name: &str, pr: u64) -> Result<()> {
    let s = store.load()?;
    if !s.tracked.contains(&pr) {
        store.track(pr)?;
    }
    let mut state = ps.load()?;
    state.feature_envelopes.insert(name.to_string());
    let rec = state.records.entry(pr).or_insert_with(|| crate::process_store::ProcessRecord {
        pr,
        expected_branch: String::new(),
        pid: None,
        feature_envelope: None,
        worktree: String::new(),
        app_config_name: None,
    });
    rec.feature_envelope = Some(name.to_string());
    ps.save_state(state)
}

pub fn feature_list(ps: &ProcessStateStore) -> Result<Vec<FeatureInfo>> {
    let state = ps.load()?;
    let mut infos: Vec<FeatureInfo> = state.feature_envelopes.iter().map(|name| {
        let prs: Vec<u64> = state.records.values()
            .filter(|r| r.feature_envelope.as_deref() == Some(name))
            .map(|r| r.pr)
            .collect();
        FeatureInfo { name: name.clone(), prs }
    }).collect();
    infos.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(infos)
}

pub fn bootstrap_pr(ps: &ProcessStateStore, config: &AppConfig, pr: u64, worktree: &Path, org: &str, repo: &str) -> Result<()> {
    let instance = format!("fp-{}-{}-{}", org, repo, pr).to_lowercase().replace('/', "-");
    let child = std::process::Command::new("sh")
        .args(["-c", &config.bootstrap])
        .current_dir(worktree)
        .env("FP_INSTANCE", &instance)
        .env("FP_WORKTREE", worktree)
        .env("FP_PR", pr.to_string())
        .env("COMPOSE_PROJECT_NAME", &instance)
        .spawn()?;
    let pid = child.id();
    ps.activate(ProcessRecord {
        pr,
        expected_branch: String::new(),
        pid: Some(pid),
        feature_envelope: None,
        worktree: worktree.to_string_lossy().to_string(),
        app_config_name: None,
    })
}

pub fn teardown_pr(ps: &ProcessStateStore, config: &AppConfig, pr: u64, worktree: &Path, org: &str, repo: &str) -> Result<()> {
    let instance = format!("fp-{}-{}-{}", org, repo, pr).to_lowercase().replace('/', "-");
    let _ = std::process::Command::new("sh")
        .args(["-c", &config.teardown])
        .current_dir(worktree)
        .env("FP_INSTANCE", &instance)
        .env("FP_WORKTREE", worktree)
        .env("FP_PR", pr.to_string())
        .env("COMPOSE_PROJECT_NAME", &instance)
        .status();
    ps.deactivate(pr)
}

pub fn health_check_branch(worktree: &Path, expected_branch: &str) -> bool {
    std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(worktree)
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim() == expected_branch)
        .unwrap_or(false)
}

pub fn health_check_pid(pid: u32) -> bool {
    if pid == 0 || pid > i32::MAX as u32 { return false; }
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

pub fn health_check_service(cmd: &str, worktree: &Path, pr: u64, fp_worktree: &Path) -> bool {
    std::process::Command::new("sh")
        .args(["-c", cmd])
        .current_dir(worktree)
        .env("FP_WORKTREE", fp_worktree)
        .env("FP_PR", pr.to_string())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn check_conflicts(ps: &ProcessStateStore, envelope_name: &str) -> Result<ConflictResult> {
    let state = ps.load()?;
    let mut blocking_prs: Vec<u64> = Vec::new();
    let mut blocking_feature = String::new();
    for record in state.records.values() {
        let env = match &record.feature_envelope {
            Some(e) if e != envelope_name => e.clone(),
            _ => continue,
        };
        let alive = record.pid.map(health_check_pid).unwrap_or(false);
        if alive {
            blocking_prs.push(record.pr);
            blocking_feature = env;
        }
    }
    if blocking_prs.is_empty() {
        Ok(ConflictResult::NoConflict)
    } else {
        Ok(ConflictResult::Conflict { blocking_feature, blocking_prs })
    }
}
