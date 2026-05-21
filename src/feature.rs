use std::path::Path;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
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

#[derive(Debug, Clone, serde::Serialize)]
pub struct PrHealthStatus {
    pub pr: u64,
    pub pid_alive: bool,
    pub service_healthy: Option<bool>,
    pub branch_ok: Option<bool>,
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
            let app_cfg = r.app_config_names.first().map(|n| n.as_str())
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
            let app_cfg = r.app_config_names.first().map(|n| n.as_str())
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

pub fn feature_add(ps: &ProcessStateStore, store: &Store, name: &str, pr: u64, configs: &[String]) -> Result<()> {
    let s = store.load()?;
    if !s.tracked.contains(&pr) {
        store.track(pr)?;
    }
    let mut state = ps.load()?;
    state.feature_envelopes.insert(name.to_string());
    let branch = s.cache.get(&pr).map(|c| c.branch.clone()).unwrap_or_default();
    let worktree_path = if !branch.is_empty() {
        crate::worktree::main_repo_root(&std::env::current_dir().unwrap_or_default())
            .ok()
            .and_then(|root| {
                let p = crate::worktree::worktree_path(&root, &branch);
                if p.exists() { Some(p.to_string_lossy().to_string()) } else { None }
            })
            .unwrap_or_default()
    } else { String::new() };
    let rec = state.records.entry(pr).or_insert_with(|| crate::process_store::ProcessRecord {
        pr,
        expected_branch: branch.clone(),
        pid: None,
        feature_envelope: None,
        worktree: worktree_path,
        app_config_names: vec![],
    });
    rec.feature_envelope = Some(name.to_string());
    for cfg in configs {
        if !rec.app_config_names.contains(cfg) {
            rec.app_config_names.push(cfg.clone());
        }
    }
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

pub fn feature_add_dep(ps: &ProcessStateStore, envelope: &str, app_config_name: &str) -> Result<()> {
    let mut state = ps.load()?;
    state.feature_envelopes.insert(envelope.to_string());
    state.envelope_deps.entry(envelope.to_string()).or_default().push(app_config_name.to_string());
    ps.save_state(state)
}

pub fn feature_up(ps: &ProcessStateStore, config: &crate::app_config::AppConfigStore, name: &str) -> Result<Vec<String>> {
    let state = ps.load()?;
    let records: Vec<_> = state.records.values()
        .filter(|r| r.feature_envelope.as_deref() == Some(name))
        .cloned()
        .collect();
    let deps = state.envelope_deps.get(name).cloned().unwrap_or_default();
    let mut messages = Vec::new();
    // Bootstrap PR members
    for rec in &records {
        if rec.app_config_names.is_empty() {
            messages.push(format!("PR #{}: no app config assigned — skipped", rec.pr));
            continue;
        }
        let worktree = std::path::Path::new(&rec.worktree);
        for cfg_name in &rec.app_config_names {
            let cfg = match config.load_app_config(cfg_name).ok().flatten() {
                Some(c) => c,
                None => { messages.push(format!("PR #{}: app config '{}' not found — skipped", rec.pr, cfg_name)); continue; }
            };
            bootstrap_pr(ps, &cfg, rec.pr, worktree, "", "")?;
            messages.push(format!("PR #{}: started ({})", rec.pr, cfg.name));
        }
    }
    // Bootstrap main-worktree instances for dep slots with no live PR member
    let live_configs: std::collections::HashSet<String> = records.iter()
        .flat_map(|r| r.app_config_names.iter().cloned())
        .collect();
    for dep_cfg_name in &deps {
        if live_configs.contains(dep_cfg_name) {
            continue;
        }
        let cfg = match config.load_app_config(dep_cfg_name).ok().flatten() {
            Some(c) => c,
            None => {
                messages.push(format!("dep {}: app config not found — skipped", dep_cfg_name));
                continue;
            }
        };
        let main_wt = match &cfg.main_worktree {
            Some(p) => p.clone(),
            None => {
                messages.push(format!("dep {}: no main_worktree configured — skipped", dep_cfg_name));
                continue;
            }
        };
        let worktree = std::path::Path::new(&main_wt);
        ps.activate(ProcessRecord {
            pr: 0,
            expected_branch: String::new(),
            pid: None,
            feature_envelope: Some(name.to_string()),
            worktree: main_wt.clone(),
            app_config_names: vec![dep_cfg_name.clone()],
        })?;
        bootstrap_pr(ps, &cfg, 0, worktree, "", "")?;
        messages.push(format!("dep {} (main): started", dep_cfg_name));
    }
    Ok(messages)
}

pub fn feature_down(ps: &ProcessStateStore, config: &crate::app_config::AppConfigStore, name: &str) -> Result<Vec<String>> {
    let state = ps.load()?;
    let records: Vec<_> = state.records.values()
        .filter(|r| r.feature_envelope.as_deref() == Some(name))
        .cloned()
        .collect();
    let mut messages = Vec::new();
    for rec in records {
        let app_cfg = rec.app_config_names.first().map(|n| n.as_str())
            .and_then(|n| config.load_app_config(n).ok().flatten());
        let cfg = match app_cfg {
            Some(c) => c,
            None => {
                messages.push(format!("PR #{}: no app config assigned — skipped", rec.pr));
                continue;
            }
        };
        let worktree = std::path::Path::new(&rec.worktree);
        teardown_pr(ps, &cfg, rec.pr, worktree, "", "")?;
        messages.push(format!("PR #{}: stopped ({})", rec.pr, cfg.name));
    }
    Ok(messages)
}

pub fn feature_rebuild(ps: &ProcessStateStore, config: &crate::app_config::AppConfigStore, name: &str, pr_filter: Option<u64>) -> Result<Vec<String>> {
    let state = ps.load()?;
    let records: Vec<_> = state.records.values()
        .filter(|r| r.feature_envelope.as_deref() == Some(name))
        .filter(|r| pr_filter.map(|p| r.pr == p).unwrap_or(true))
        .cloned()
        .collect();
    let mut messages = Vec::new();
    for rec in records {
        let app_cfg = rec.app_config_names.first().map(|n| n.as_str())
            .and_then(|n| config.load_app_config(n).ok().flatten());
        let cfg = match app_cfg {
            Some(c) => c,
            None => {
                messages.push(format!("PR #{}: no app config assigned — skipped", rec.pr));
                continue;
            }
        };
        if !cfg.ephemeral {
            messages.push(format!("PR #{}: uses a persistent app config '{}' — use `fp feature down` + `fp feature up` instead", rec.pr, cfg.name));
            continue;
        }
        let worktree = std::path::Path::new(&rec.worktree);
        let instance = format!("fp---{}", rec.pr);
        std::process::Command::new("sh")
            .args(["-c", &cfg.bootstrap])
            .current_dir(worktree)
            .env("FP_INSTANCE", &instance)
            .env("FP_WORKTREE", worktree)
            .env("FP_PR", rec.pr.to_string())
            .env("COMPOSE_PROJECT_NAME", &instance)
            .status()?;
        messages.push(format!("PR #{}: rebuilt ({})", rec.pr, cfg.name));
    }
    Ok(messages)
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
        .process_group(0)
        .spawn()?;
    let pid = child.id();
    ps.activate(ProcessRecord {
        pr,
        expected_branch: String::new(),
        pid: Some(pid),
        feature_envelope: None,
        worktree: worktree.to_string_lossy().to_string(),
        app_config_names: vec![],
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

pub fn health_check_branch(worktree: &Path, expected_branch: &str) -> Option<bool> {
    if !worktree.exists() || expected_branch.is_empty() {
        return None;
    }
    std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(worktree)
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim() == expected_branch)
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
