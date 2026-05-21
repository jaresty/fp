use std::process::Command;
use tempfile::TempDir;

fn setup_repo() -> TempDir {
    let dir = TempDir::new().unwrap();
    let p = dir.path();
    let git = |args: &[&str]| Command::new("git").args(args).current_dir(p).output().unwrap();
    git(&["init", "-b", "main"]);
    git(&["config", "user.email", "t@t.com"]);
    git(&["config", "user.name", "T"]);
    std::fs::write(p.join("f"), "x").unwrap();
    git(&["add", "."]);
    git(&["commit", "-m", "init"]);
    dir
}

fn fp(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_fp"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to run fp")
}

/// fp switch --non-interactive must be a recognized flag (clap accepts it)
#[test]
fn cli_switch_non_interactive_flag_is_recognized() {
    let dir = setup_repo();
    // PR 999 is untracked — the command will fail, but NOT with "unrecognized argument"
    let out = fp(dir.path(), &["switch", "999", "sess", "--non-interactive"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains("unrecognized argument"),
        "--non-interactive must be a recognized flag, got: {}", stderr);
    assert!(!stderr.contains("unexpected argument"),
        "--non-interactive must be a recognized flag, got: {}", stderr);
}

/// fp feature up --no must be a recognized flag
#[test]
fn cli_feature_up_no_flag_is_recognized() {
    let dir = setup_repo();
    let out = fp(dir.path(), &["feature", "up", "nonexistent-feature", "--no"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains("unrecognized argument"),
        "--no must be a recognized flag, got: {}", stderr);
    assert!(!stderr.contains("unexpected argument"),
        "--no must be a recognized flag, got: {}", stderr);
}

/// fp pr up --config must be a recognized flag
#[test]
fn cli_pr_up_config_flag_is_recognized() {
    let dir = setup_repo();
    let out = fp(dir.path(), &["pr", "up", "999", "--config", "my-app"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains("unrecognized argument"),
        "--config must be a recognized flag, got: {}", stderr);
    assert!(!stderr.contains("unexpected argument"),
        "--config must be a recognized flag, got: {}", stderr);
}

/// fp switch must update the process record worktree path so feature status can check the branch
#[test]
fn cli_switch_updates_process_record_worktree_path() {
    let dir = setup_repo();
    let p = dir.path();
    let git = |args: &[&str]| std::process::Command::new("git").args(args).current_dir(p).output().unwrap();
    git(&["checkout", "-b", "feat/x"]);
    let fp_bin = env!("CARGO_BIN_EXE_fp");
    let fp = |args: &[&str]| std::process::Command::new(fp_bin).args(args).current_dir(p).output().unwrap();
    fp(&["track", "10", "--title", "X", "--branch", "feat/x"]);
    fp(&["feature", "new", "my-feat"]);
    fp(&["feature", "add", "my-feat", "10"]);
    // Check process record worktree is empty before switch
    let state_before: serde_json::Value = serde_json::from_slice(
        &std::fs::read(p.join(".git/fp/process-state.json")).unwrap()
    ).unwrap();
    assert_eq!(state_before["records"]["10"]["worktree"].as_str().unwrap_or(""), "",
        "worktree must be empty before fp switch");
    // fp switch with --adopt (branch is current HEAD)
    let switch_out = fp(&["switch", "10", "sess", "--adopt"]);
    assert!(switch_out.status.success(), "fp switch must succeed: {}", String::from_utf8_lossy(&switch_out.stderr));
    let stdout = String::from_utf8_lossy(&switch_out.stdout);
    let wt_path = stdout.lines().last().unwrap_or("").trim().to_string();
    // Check process record worktree is updated after switch
    let state_after: serde_json::Value = serde_json::from_slice(
        &std::fs::read(p.join(".git/fp/process-state.json")).unwrap()
    ).unwrap();
    let recorded_wt = state_after["records"]["10"]["worktree"].as_str().unwrap_or("");
    assert_eq!(recorded_wt, wt_path,
        "process record worktree must match fp switch output path after switch");
}

/// fp app list must be a recognized subcommand
#[test]
fn cli_app_list_is_recognized() {
    let dir = setup_repo();
    let out = fp(dir.path(), &["app", "list"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains("unrecognized subcommand"),
        "app list must be a recognized subcommand, got: {}", stderr);
    assert!(!stderr.contains("unexpected argument"),
        "app list must be a recognized subcommand, got: {}", stderr);
}

/// fp feature test must be a recognized subcommand
#[test]
fn cli_feature_test_is_recognized() {
    let dir = setup_repo();
    let out = fp(dir.path(), &["feature", "test", "nonexistent"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains("unrecognized subcommand"),
        "feature test must be a recognized subcommand, got: {}", stderr);
    assert!(!stderr.contains("unexpected argument"),
        "feature test must be a recognized subcommand, got: {}", stderr);
}

/// fp feature set-test must be a recognized subcommand
#[test]
fn cli_feature_set_test_is_recognized() {
    let dir = setup_repo();
    let out = fp(dir.path(), &["feature", "set-test", "nonexistent", "echo ok"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains("unrecognized subcommand"),
        "feature set-test must be a recognized subcommand, got: {}", stderr);
    assert!(!stderr.contains("unexpected argument"),
        "feature set-test must be a recognized subcommand, got: {}", stderr);
}

/// fp feature remove-dep must be a recognized subcommand
#[test]
fn cli_feature_remove_dep_is_recognized() {
    let dir = setup_repo();
    let out = fp(dir.path(), &["feature", "remove-dep", "nonexistent"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains("unrecognized subcommand"),
        "feature remove-dep must be a recognized subcommand, got: {}", stderr);
    assert!(!stderr.contains("unexpected argument"),
        "feature remove-dep must be a recognized subcommand, got: {}", stderr);
}

/// fp feature logs must be a recognized subcommand
#[test]
fn cli_feature_logs_is_recognized() {
    let dir = setup_repo();
    let out = fp(dir.path(), &["feature", "logs", "nonexistent"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains("unrecognized subcommand"),
        "feature logs must be a recognized subcommand, got: {}", stderr);
    assert!(!stderr.contains("unexpected argument"),
        "feature logs must be a recognized subcommand, got: {}", stderr);
}

/// fp feature status --json must be a recognized flag
#[test]
fn cli_feature_status_json_flag_is_recognized() {
    let dir = setup_repo();
    let out = fp(dir.path(), &["feature", "status", "nonexistent", "--json"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains("unrecognized argument"),
        "--json must be a recognized flag, got: {}", stderr);
    assert!(!stderr.contains("unexpected argument"),
        "--json must be a recognized flag, got: {}", stderr);
}
