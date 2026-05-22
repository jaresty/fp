use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::process::Stdio;
#[cfg(unix)]
extern crate libc;

fn spawn_daemon(cmd: &str, dir: &Path, envs: &[(&str, &str)], log_path: &Path) -> Result<std::process::Child> {
    let log_file = std::fs::OpenOptions::new()
        .create(true).append(true).open(log_path)?;
    let log_err = log_file.try_clone()?;
    let mut command = std::process::Command::new("sh");
    command.args(["-c", cmd])
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(log_err));
    for (k, v) in envs {
        command.env(k, v);
    }
    #[cfg(unix)]
    unsafe { command.pre_exec(|| { libc::setsid(); Ok(()) }); }
    Ok(command.spawn()?)
}

pub fn resolve_worktree(repo_root: &Path, branch: &str) -> PathBuf {
    if branch.is_empty() { repo_root.to_path_buf() }
    else { crate::worktree::worktree_path(repo_root, branch) }
}
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

pub fn feature_list_running_with_config(ps: &ProcessStateStore, config: &crate::app_config::AppConfigStore, repo_root: &std::path::Path) -> Result<Vec<FeatureInfo>> {
    let state = ps.load()?;
    let running: Vec<FeatureInfo> = state.feature_envelopes.iter().filter(|name| {
        state.records.values().any(|r| {
            if r.feature_envelope.as_deref() != Some(name) { return false; }
            let app_cfg = r.app_config_names.first().map(|n| n.as_str())
                .and_then(|n| config.load_app_config(n).ok().flatten());
            let is_ephemeral = app_cfg.as_ref().map(|c| c.ephemeral).unwrap_or(false);
            if is_ephemeral {
                let worktree = resolve_worktree(repo_root, &r.expected_branch);
                app_cfg.and_then(|c| c.health_check)
                    .map(|cmd| health_check_service(&cmd, &worktree, r.pr, &worktree))
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

pub fn feature_status(ps: &ProcessStateStore, config: &crate::app_config::AppConfigStore, name: &str, repo_root: &std::path::Path) -> Result<Vec<PrHealthStatus>> {
    let state = ps.load()?;
    let mut statuses: Vec<PrHealthStatus> = state.records.values()
        .filter(|r| r.feature_envelope.as_deref() == Some(name))
        .map(|r| {
            let worktree = resolve_worktree(repo_root, &r.expected_branch);
            let branch_ok = health_check_branch(&worktree, &r.expected_branch);
            let app_cfg = r.app_config_names.first().map(|n| n.as_str())
                .and_then(|n| config.load_app_config(n).ok().flatten());
            let is_ephemeral = app_cfg.as_ref().map(|c| c.ephemeral).unwrap_or(false);
            if is_ephemeral {
                let service_healthy = app_cfg.and_then(|c| c.health_check)
                    .map(|cmd| health_check_service(&cmd, &worktree, r.pr, &worktree));
                PrHealthStatus { pr: r.pr, pid_alive: false, service_healthy, branch_ok }
            } else {
                let pid_alive = r.pid.map(health_check_pid).unwrap_or(false);
                let service_healthy = app_cfg.and_then(|c| c.health_check)
                    .map(|cmd| health_check_service(&cmd, &worktree, r.pr, &worktree));
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

pub fn feature_up(ps: &ProcessStateStore, config: &crate::app_config::AppConfigStore, name: &str, repo_root: &std::path::Path) -> Result<Vec<String>> {
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
        let worktree_buf = resolve_worktree(repo_root, &rec.expected_branch);
        let worktree = worktree_buf.as_path();
        for cfg_name in &rec.app_config_names {
            let cfg = match config.load_app_config(cfg_name).ok().flatten() {
                Some(c) => c,
                None => { messages.push(format!("PR #{}: app config '{}' not found — skipped", rec.pr, cfg_name)); continue; }
            };
            let pid_alive = rec.pid.map(health_check_pid).unwrap_or(false);
            if !pid_alive {
                let svc_healthy = cfg.health_check.as_deref()
                    .map(|cmd| health_check_service(cmd, worktree, rec.pr, worktree))
                    .unwrap_or(false);
                if svc_healthy {
                    anyhow::bail!("PR #{}: app '{}' is healthy but untracked — another process may be listening; cannot start. Stop it first or use `fp feature down` then retry.", rec.pr, cfg.name);
                }
            }
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
        let worktree = repo_root;
        let worktree_str = repo_root.to_string_lossy().to_string();
        let instance = format!("fp-dep-{}-{}", name, dep_cfg_name).to_lowercase().replace('/', "-");
        let log_dir = ps.path.parent().unwrap_or(Path::new(".")).join("logs");
        std::fs::create_dir_all(&log_dir)?;
        let log_path = log_dir.join(format!("{}.log", instance));
        let envs: &[(&str, &str)] = &[
            ("FP_INSTANCE", &instance),
            ("FP_WORKTREE", &worktree_str),
            ("FP_PR", "0"),
            ("COMPOSE_PROJECT_NAME", &instance),
        ];
        let child = spawn_daemon(&cfg.bootstrap, worktree, envs, &log_path)?;
        let pid = child.id();
        let key = format!("{}:{}", name, dep_cfg_name);
        let mut state = ps.load()?;
        state.dep_records.insert(key, crate::process_store::DepRecord {
            app_config_name: dep_cfg_name.clone(),
            feature_envelope: name.to_string(),
            pid: Some(pid),
            worktree: worktree_str,
        });
        ps.save_state(state)?;
        messages.push(format!("dep {} (main): started", dep_cfg_name));
    }
    Ok(messages)
}

pub fn feature_down(ps: &ProcessStateStore, config: &crate::app_config::AppConfigStore, name: &str, repo_root: &std::path::Path) -> Result<Vec<String>> {
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
        let worktree_buf = resolve_worktree(repo_root, &rec.expected_branch);
        let worktree = worktree_buf.as_path();
        teardown_pr(ps, &cfg, rec.pr, worktree, "", "")?;
        messages.push(format!("PR #{}: stopped ({})", rec.pr, cfg.name));
    }
    // Tear down dep slots for this envelope
    let dep_keys: Vec<String> = {
        let state = ps.load()?;
        state.dep_records.keys()
            .filter(|k| k.starts_with(&format!("{}:", name)))
            .cloned()
            .collect()
    };
    for key in &dep_keys {
        let dep_cfg_name = key.split_once(':').map(|x| x.1).unwrap_or("").to_string();
        let dep_state = ps.load()?;
        if let Some(dep_rec) = dep_state.dep_records.get(key) {
            let worktree = std::path::Path::new(&dep_rec.worktree);
            if let Some(cfg) = config.load_app_config(&dep_cfg_name).ok().flatten() {
                let instance = format!("fp-dep-{}-{}", name, dep_cfg_name).to_lowercase().replace('/', "-");
                let _ = std::process::Command::new("sh")
                    .args(["-c", &cfg.teardown])
                    .current_dir(worktree)
                    .env("FP_INSTANCE", &instance)
                    .env("FP_WORKTREE", &dep_rec.worktree)
                    .env("FP_PR", "0")
                    .env("COMPOSE_PROJECT_NAME", &instance)
                    .status();
                messages.push(format!("dep {} (main): stopped", dep_cfg_name));
            }
        }
        let mut s = ps.load()?;
        s.dep_records.remove(key);
        ps.save_state(s)?;
    }
    Ok(messages)
}

pub fn feature_rebuild(ps: &ProcessStateStore, config: &crate::app_config::AppConfigStore, name: &str, pr_filter: Option<u64>, repo_root: &std::path::Path) -> Result<Vec<String>> {
    let mut messages = Vec::new();
    // Rebuild dep slots when pr_filter is Some(0) or None (all)
    if pr_filter == Some(0) || pr_filter.is_none() {
        let dep_keys: Vec<String> = {
            let state = ps.load()?;
            state.dep_records.keys()
                .filter(|k| k.starts_with(&format!("{}:", name)))
                .cloned()
                .collect()
        };
        for key in &dep_keys {
            let dep_cfg_name = key.split_once(':').map(|x| x.1).unwrap_or("").to_string();
            let cfg = match config.load_app_config(&dep_cfg_name).ok().flatten() {
                Some(c) => c,
                None => { messages.push(format!("dep {}: app config not found — skipped", dep_cfg_name)); continue; }
            };
            let worktree_str = repo_root.to_string_lossy().to_string();
            let instance = format!("fp-dep-{}-{}", name, dep_cfg_name).to_lowercase().replace('/', "-");
            let child = std::process::Command::new("sh")
                .args(["-c", &cfg.bootstrap])
                .current_dir(repo_root)
                .env("FP_INSTANCE", &instance)
                .env("FP_WORKTREE", &worktree_str)
                .env("FP_PR", "0")
                .env("COMPOSE_PROJECT_NAME", &instance)
                .process_group(0)
                .spawn()?;
            let pid = child.id();
            let mut state = ps.load()?;
            if let Some(rec) = state.dep_records.get_mut(key) {
                rec.pid = Some(pid);
            }
            ps.save_state(state)?;
            messages.push(format!("dep {} (main): rebuilt", dep_cfg_name));
        }
        if pr_filter == Some(0) {
            return Ok(messages);
        }
    }
    let state = ps.load()?;
    let records: Vec<_> = state.records.values()
        .filter(|r| r.feature_envelope.as_deref() == Some(name))
        .filter(|r| pr_filter.map(|p| r.pr == p).unwrap_or(true))
        .cloned()
        .collect();
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
        let worktree_buf = resolve_worktree(repo_root, &rec.expected_branch);
        let worktree = worktree_buf.as_path();
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
    let log_dir = ps.path.parent().unwrap_or(Path::new(".")).join("logs");
    std::fs::create_dir_all(&log_dir)?;
    let log_path = log_dir.join(format!("{}.log", instance));
    let pr_str = pr.to_string();
    let worktree_str = worktree.to_string_lossy();
    let envs: &[(&str, &str)] = &[
        ("FP_INSTANCE", &instance),
        ("FP_WORKTREE", &worktree_str),
        ("FP_PR", &pr_str),
        ("COMPOSE_PROJECT_NAME", &instance),
    ];
    let child = spawn_daemon(&config.bootstrap, worktree, envs, &log_path)?;
    let pid = child.id();
    let mut state = ps.load()?;
    let rec = state.records.entry(pr).or_insert_with(|| ProcessRecord {
        pr,
        expected_branch: String::new(),
        pid: None,
        feature_envelope: None,
        worktree: String::new(),
        app_config_names: vec![],
    });
    rec.pid = Some(pid);
    rec.worktree = worktree.to_string_lossy().to_string();
    ps.save_state(state)
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
    let mut state = ps.load()?;
    if let Some(rec) = state.records.get_mut(&pr) {
        rec.pid = None;
    }
    ps.save_state(state)
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
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
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
