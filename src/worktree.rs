use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockInfo {
    pub pid: u32,
    pub kind: String,
}

/// Returns the common (main) git dir for the repo, even when called from inside a worktree.
/// Equivalent to `git rev-parse --git-common-dir`.
pub fn common_git_dir(cwd: &Path) -> anyhow::Result<PathBuf> {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "--git-common-dir"])
        .current_dir(cwd)
        .output()?;
    anyhow::ensure!(out.status.success(), "not in a git repository");
    let raw = PathBuf::from(String::from_utf8(out.stdout)?.trim().to_string());
    // git may return a relative path; resolve it against cwd
    Ok(if raw.is_absolute() { raw } else { cwd.join(raw) })
}

/// Returns the worktree directory for a branch: sibling to repo root named `<repo>-worktrees/<branch>`.
pub fn worktree_path(repo_root: &Path, branch: &str) -> PathBuf {
    let name = repo_root.file_name().unwrap_or_default().to_string_lossy();
    repo_root.parent().unwrap_or(repo_root)
        .join(format!("{}-worktrees", name))
        .join(branch)
}

/// Returns the lock file path inside .git/worktrees/<branch>/fp-lock.
pub fn lock_path(git_dir: &Path, branch: &str) -> PathBuf {
    git_dir.join("worktrees").join(branch).join("fp-lock")
}

/// Reads and parses a lock file. Returns None if absent or unparseable.
pub fn read_lock(lock_path: &Path) -> Option<LockInfo> {
    let data = std::fs::read_to_string(lock_path).ok()?;
    serde_json::from_str(&data).ok()
}

/// Returns true if the process with the given PID is running.
pub fn pid_is_alive(pid: u32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Returns true if the lock represents a live (non-stale) occupant.
pub fn lock_is_live(lock: &LockInfo) -> bool {
    pid_is_alive(lock.pid)
}

/// Writes a lock file at the given path, creating parent dirs as needed.
pub fn write_lock(lock_path: &Path, pid: u32, kind: &str) -> anyhow::Result<()> {
    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let info = LockInfo { pid, kind: kind.to_string() };
    std::fs::write(lock_path, serde_json::to_string(&info)?)?;
    Ok(())
}

/// Removes the lock file at the given path. Silently ignores if absent.
pub fn remove_lock(lock_path: &Path) -> anyhow::Result<()> {
    if lock_path.exists() { std::fs::remove_file(lock_path)?; }
    Ok(())
}

/// Returns true if the git repo at `repo_root` has uncommitted changes.
pub fn repo_is_dirty(repo_root: &Path) -> anyhow::Result<bool> {
    let out = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(repo_root)
        .output()?;
    Ok(!out.stdout.is_empty())
}

/// Checks the target worktree lock; clears stale locks. Returns Err if a live lock exists.
pub fn check_target_lock(git_dir: &Path, branch: &str) -> anyhow::Result<()> {
    let lp = lock_path(git_dir, branch);
    if let Some(lock) = read_lock(&lp) {
        if lock_is_live(&lock) {
            anyhow::bail!("worktree for '{}' is locked by PID {} ({})", branch, lock.pid, lock.kind);
        }
        // Stale lock — remove it
        remove_lock(&lp)?;
    }
    Ok(())
}

/// Returns a short lock status string for display in watch/status output.
/// Returns None if no lock file exists or lock is stale.
pub fn lock_status(git_dir: &Path, branch: &str) -> Option<String> {
    let lp = lock_path(git_dir, branch);
    let lock = read_lock(&lp)?;
    if !lock_is_live(&lock) {
        return None;
    }
    let label = if lock.pid == std::process::id() {
        "you".to_string()
    } else {
        format!("{} (pid {})", lock.kind, lock.pid)
    };
    Some(format!("🔒 {}", label))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_repo(name: &str) -> (TempDir, PathBuf, PathBuf) {
        let base = TempDir::new().unwrap();
        let repo = base.path().join(name);
        fs::create_dir_all(&repo).unwrap();
        let git_dir = repo.join(".git");
        fs::create_dir_all(&git_dir).unwrap();
        (base, repo, git_dir)
    }

    #[test]
    fn worktree_path_is_sibling_to_repo() {
        let (_base, repo, _git) = make_repo("myrepo");
        let path = worktree_path(&repo, "feat/auth");
        assert_eq!(
            path,
            repo.parent().unwrap().join("myrepo-worktrees").join("feat/auth"),
            "worktree_path must be sibling to repo at <repo>/../<repo>-worktrees/<branch>"
        );
    }

    #[test]
    fn lock_path_is_inside_git_worktrees() {
        let (_base, _repo, git_dir) = make_repo("myrepo");
        let path = lock_path(&git_dir, "feat/auth");
        assert_eq!(
            path,
            git_dir.join("worktrees").join("feat/auth").join("fp-lock"),
            "lock_path must be .git/worktrees/<branch>/fp-lock"
        );
    }

    #[test]
    fn write_and_read_lock_roundtrip() {
        let dir = TempDir::new().unwrap();
        let lp = dir.path().join("worktrees").join("branch").join("fp-lock");
        write_lock(&lp, 12345, "agent").unwrap();
        let lock = read_lock(&lp).expect("lock should be readable after write");
        assert_eq!(lock.pid, 12345);
        assert_eq!(lock.kind, "agent");
    }

    #[test]
    fn remove_lock_cleans_up() {
        let dir = TempDir::new().unwrap();
        let lp = dir.path().join("fp-lock");
        write_lock(&lp, 9999, "watch").unwrap();
        assert!(lp.exists());
        remove_lock(&lp).unwrap();
        assert!(!lp.exists());
    }

    #[test]
    fn pid_is_alive_current_process() {
        assert!(pid_is_alive(std::process::id()), "current process must be detected as alive");
    }

    #[test]
    fn pid_is_alive_nonexistent_process() {
        assert!(!pid_is_alive(999_999_999), "nonexistent PID must not be alive");
    }

    #[test]
    fn lock_is_live_with_current_pid() {
        let lock = LockInfo { pid: std::process::id(), kind: "agent".into() };
        assert!(lock_is_live(&lock));
    }

    #[test]
    fn lock_is_live_with_dead_pid() {
        let lock = LockInfo { pid: 999_999_999, kind: "agent".into() };
        assert!(!lock_is_live(&lock));
    }

    #[test]
    fn common_git_dir_returns_main_git_from_worktree() {
        let base = TempDir::new().unwrap();
        let repo = base.path().join("myrepo");
        fs::create_dir_all(&repo).unwrap();
        std::process::Command::new("git").args(["init"]).current_dir(&repo).output().unwrap();
        std::process::Command::new("git").args(["config", "user.email", "t@t.com"]).current_dir(&repo).output().unwrap();
        std::process::Command::new("git").args(["config", "user.name", "T"]).current_dir(&repo).output().unwrap();
        // Need at least one commit before creating a worktree
        fs::write(repo.join("f.txt"), "x").unwrap();
        std::process::Command::new("git").args(["add", "."]).current_dir(&repo).output().unwrap();
        std::process::Command::new("git").args(["commit", "-m", "init"]).current_dir(&repo).output().unwrap();
        std::process::Command::new("git").args(["branch", "feat"]).current_dir(&repo).output().unwrap();
        let wt = base.path().join("wt");
        std::process::Command::new("git").args(["worktree", "add", wt.to_str().unwrap(), "feat"]).current_dir(&repo).output().unwrap();

        let from_main = common_git_dir(&repo).expect("common_git_dir must succeed from main repo");
        let from_wt = common_git_dir(&wt).expect("common_git_dir must succeed from worktree");

        // Both should resolve to the same directory (the main .git)
        assert_eq!(
            fs::canonicalize(&from_main).unwrap(),
            fs::canonicalize(&from_wt).unwrap(),
            "common_git_dir must return the same main .git from both main repo and worktree"
        );
    }

    #[test]
    fn dirty_check_detects_dirty_worktree_path() {
        let dir = TempDir::new().unwrap();
        // Init a git repo in the tempdir
        std::process::Command::new("git").args(["init"]).current_dir(dir.path()).output().unwrap();
        std::process::Command::new("git").args(["config", "user.email", "t@t.com"]).current_dir(dir.path()).output().unwrap();
        std::process::Command::new("git").args(["config", "user.name", "T"]).current_dir(dir.path()).output().unwrap();
        // Create and stage a file to make it dirty
        fs::write(dir.path().join("file.txt"), "hello").unwrap();
        let is_dirty = repo_is_dirty(dir.path()).expect("repo_is_dirty must succeed on valid git repo");
        assert!(is_dirty, "dirty_check must return true for a path with uncommitted changes");
    }

    #[test]
    fn dirty_check_returns_false_for_clean_repo() {
        let dir = TempDir::new().unwrap();
        std::process::Command::new("git").args(["init"]).current_dir(dir.path()).output().unwrap();
        std::process::Command::new("git").args(["config", "user.email", "t@t.com"]).current_dir(dir.path()).output().unwrap();
        std::process::Command::new("git").args(["config", "user.name", "T"]).current_dir(dir.path()).output().unwrap();
        // Commit a file so HEAD exists and tree is clean
        fs::write(dir.path().join("file.txt"), "hello").unwrap();
        std::process::Command::new("git").args(["add", "."]).current_dir(dir.path()).output().unwrap();
        std::process::Command::new("git").args(["commit", "-m", "init"]).current_dir(dir.path()).output().unwrap();
        let is_dirty = repo_is_dirty(dir.path()).expect("repo_is_dirty must succeed");
        assert!(!is_dirty, "dirty_check must return false for a clean repo path");
    }

    #[test]
    fn check_target_lock_passes_when_no_lock() {
        let (_base, _repo, git_dir) = make_repo("myrepo");
        check_target_lock(&git_dir, "feat/auth").expect("no lock should pass");
    }

    #[test]
    fn check_target_lock_clears_stale_lock() {
        let (_base, _repo, git_dir) = make_repo("myrepo");
        let lp = lock_path(&git_dir, "feat/auth");
        write_lock(&lp, 999_999_999, "agent").unwrap();
        check_target_lock(&git_dir, "feat/auth").expect("stale lock should be cleared");
        assert!(!lp.exists(), "stale lock file must be removed");
    }

    #[test]
    fn check_target_lock_fails_when_live_lock() {
        let (_base, _repo, git_dir) = make_repo("myrepo");
        let lp = lock_path(&git_dir, "feat/auth");
        write_lock(&lp, std::process::id(), "agent").unwrap();
        let result = check_target_lock(&git_dir, "feat/auth");
        assert!(result.is_err(), "live lock must block switch");
        assert!(result.unwrap_err().to_string().contains("locked by PID"),
            "error must mention 'locked by PID'");
    }

    #[test]
    fn lock_status_none_when_no_lock() {
        let (_base, _repo, git_dir) = make_repo("myrepo");
        assert!(lock_status(&git_dir, "feat/auth").is_none());
    }

    #[test]
    fn lock_status_shows_you_for_current_pid() {
        let (_base, _repo, git_dir) = make_repo("myrepo");
        let lp = lock_path(&git_dir, "feat/auth");
        write_lock(&lp, std::process::id(), "agent").unwrap();
        let status = lock_status(&git_dir, "feat/auth").expect("lock status must be present");
        assert!(status.contains("you"), "lock status for current pid must say 'you', got: {}", status);
    }

    #[test]
    fn lock_status_shows_pid_for_other_process() {
        let (_base, _repo, git_dir) = make_repo("myrepo");
        let lp = lock_path(&git_dir, "feat/auth");
        // Spawn a real child process and use its PID so kill -0 succeeds
        let mut child = std::process::Command::new("sleep").arg("10").spawn().unwrap();
        let child_pid = child.id();
        write_lock(&lp, child_pid, "agent").unwrap();
        let status = lock_status(&git_dir, "feat/auth").expect("lock status must be present");
        child.kill().ok();
        assert!(status.contains(&format!("pid {}", child_pid)),
            "lock status for other pid must show pid, got: {}", status);
    }
}
