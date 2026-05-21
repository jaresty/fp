use std::process::Command;
use tempfile::TempDir;

fn setup_repo() -> TempDir {
    let dir = TempDir::new().unwrap();
    let p = dir.path();
    let git = |args: &[&str]| Command::new("git").args(args).current_dir(p).output().unwrap();
    git(&["init"]);
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
