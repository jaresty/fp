use std::process::Command;
use tempfile::TempDir;

#[test]
fn install_skills_writes_to_home_dir() {
    let dir = TempDir::new().unwrap();
    let path = dir.path();

    let git = |args: &[&str]| {
        Command::new("git").args(args).current_dir(path).output().unwrap()
    };
    git(&["init"]);
    git(&["config", "user.email", "t@t.com"]);
    git(&["config", "user.name", "T"]);
    std::fs::write(path.join("x"), "x").unwrap();
    git(&["add", "."]);
    git(&["commit", "-m", "init"]);

    let fp_bin = env!("CARGO_BIN_EXE_fp");

    let out = Command::new(fp_bin)
        .arg("install-skills")
        .current_dir(path)
        .output()
        .expect("failed to run fp");

    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));

    let home = dirs::home_dir().expect("no home dir");
    let skill_path = home.join(".claude").join("skills").join("fp").join("SKILL.md");
    assert!(skill_path.exists(), "SKILL.md not created at {}", skill_path.display());
    let content = std::fs::read_to_string(&skill_path).unwrap();
    assert!(content.contains("name: fp"), "SKILL.md missing frontmatter");
}

#[test]
fn install_skills_respects_path_override() {
    let dir = TempDir::new().unwrap();
    let path = dir.path();

    let git = |args: &[&str]| {
        Command::new("git").args(args).current_dir(path).output().unwrap()
    };
    git(&["init"]);
    git(&["config", "user.email", "t@t.com"]);
    git(&["config", "user.name", "T"]);
    std::fs::write(path.join("x"), "x").unwrap();
    git(&["add", "."]);
    git(&["commit", "-m", "init"]);

    let fp_bin = env!("CARGO_BIN_EXE_fp");
    let custom_path = path.join("custom").join("SKILL.md");

    let out = Command::new(fp_bin)
        .args(["install-skills", "--path", custom_path.to_str().unwrap()])
        .current_dir(path)
        .output()
        .expect("failed to run fp");

    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert!(custom_path.exists(), "SKILL.md not at custom path");
    let content = std::fs::read_to_string(&custom_path).unwrap();
    assert!(content.contains("name: fp"));
}

#[test]
fn cargo_husky_pre_commit_hook_exists_and_runs_clippy() {
    let hook = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".cargo-husky")
        .join("hooks")
        .join("pre-commit");
    assert!(hook.exists(), ".cargo-husky/hooks/pre-commit not found at {:?}", hook);
    let content = std::fs::read_to_string(&hook).unwrap();
    assert!(content.contains("cargo clippy"),
        "pre-commit hook should run cargo clippy, got: {}", content);
}
