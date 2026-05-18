use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockInfo {
    pub pid: u32,
    pub kind: String,
    pub id: String,
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
    // git may return a relative path; resolve and canonicalize so callers get an absolute path with no ".." components
    let joined = if raw.is_absolute() { raw } else { cwd.join(raw) };
    Ok(joined.canonicalize().unwrap_or(joined))
}

/// Returns the main repo root, even when called from inside a worktree.
/// Derived as the parent of common_git_dir(), so it always points to the main checkout.
pub fn main_repo_root(cwd: &Path) -> anyhow::Result<PathBuf> {
    let git_dir = common_git_dir(cwd)?;
    git_dir.parent()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| anyhow::anyhow!("git common dir has no parent"))
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

const KNOWN_SHELLS: &[&str] = &["bash", "sh", "zsh", "fish", "dash", "ksh", "tcsh", "csh"];

fn process_has_tty(pid: u32) -> bool {
    std::process::Command::new("ps")
        .args(["-o", "tty=", "-p", &pid.to_string()])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| {
            let t = s.trim();
            !t.is_empty() && t != "??" && t != "?"
        })
        .unwrap_or(false)
}

fn process_comm(pid: u32) -> Option<String> {
    std::process::Command::new("ps")
        .args(["-o", "comm=", "-p", &pid.to_string()])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
}

fn ppid_of(pid: u32) -> Option<u32> {
    std::process::Command::new("ps")
        .args(["-o", "ppid=", "-p", &pid.to_string()])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse().ok())
}

fn is_known_shell(comm: &str) -> bool {
    let base = std::path::Path::new(comm).file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(comm);
    KNOWN_SHELLS.contains(&base)
}

/// Returns the PID of the session anchor — the first ancestor that has a TTY or is not a known
/// shell binary, making it durable across both terminal and agent (e.g. Claude) contexts.
pub fn session_anchor_pid() -> u32 {
    let mut candidate = ppid_of(std::process::id()).unwrap_or(std::process::id());
    for _ in 0..16 {
        if process_has_tty(candidate) {
            return candidate;
        }
        if let Some(comm) = process_comm(candidate)
            && !is_known_shell(&comm) {
            return candidate;
        }
        match ppid_of(candidate) {
            Some(p) if p != candidate && p > 1 => candidate = p,
            _ => return candidate,
        }
    }
    candidate
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
pub fn write_lock(lock_path: &Path, pid: u32, kind: &str, id: &str) -> anyhow::Result<()> {
    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let info = LockInfo { pid, kind: kind.to_string(), id: id.to_string() };
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

/// Checks the target worktree lock. Returns Err if any lock exists (live or stale).
/// Callers must explicitly run `fp unlock <branch>` to clear a lock.
pub fn check_target_lock(git_dir: &Path, branch: &str) -> anyhow::Result<()> {
    let lp = lock_path(git_dir, branch);
    if let Some(lock) = read_lock(&lp) {
        let is_live = lock_is_live(&lock);
        let liveness = if is_live { "alive" } else { "dead" };
        let my_pid = session_anchor_pid();
        let safety_note = if is_live {
            format!(
                "The lock is ALIVE (pid {} is running). \
                It is safe to steal only if you have confirmed that process has finished \
                all work in this worktree and will not make further changes.",
                lock.pid
            )
        } else {
            "The lock is DEAD (pid is no longer running). It is safe to steal.".to_string()
        };
        anyhow::bail!(
            "worktree for '{}' is locked by '{}' (pid {}, {})\n\n\
            Your anchor PID is {}. The holder is {}.\n\n\
            Stealing this lock means both processes could access the same worktree \
            simultaneously, risking lost or corrupted uncommitted changes.\n\n\
            {}\n\n\
            To steal: fp unlock {}",
            branch, lock.id, lock.pid, liveness,
            my_pid,
            if lock.pid == my_pid { "YOU (same process)" } else { "a DIFFERENT process" },
            safety_note,
            branch
        );
    }
    Ok(())
}

/// Returns a short lock status string for display in watch/status output.
/// Returns None only if no lock file exists. Shows alive/dead for all locks.
/// Parse `git worktree list --porcelain` output and return the checkout path for `branch`.
pub fn parse_worktree_list(output: &str, branch: &str) -> Option<PathBuf> {
    let target = format!("refs/heads/{}", branch);
    let mut current_path: Option<PathBuf> = None;
    for line in output.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            current_path = Some(PathBuf::from(path));
        } else if line == format!("branch {}", target) {
            return current_path;
        }
    }
    None
}

/// Find the worktree directory where `branch` is currently checked out, if any.
pub fn find_worktree_path(branch: &str, repo_root: &Path) -> Option<PathBuf> {
    let out = std::process::Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(repo_root)
        .output().ok()?;
    let text = String::from_utf8(out.stdout).ok()?;
    parse_worktree_list(&text, branch)
}

pub fn lock_status(git_dir: &Path, branch: &str) -> Option<String> {
    let lp = lock_path(git_dir, branch);
    let lock = read_lock(&lp)?;
    let is_live = lock_is_live(&lock);
    let liveness = if is_live { "alive — do not steal" } else { "dead — safe to steal with: fp unlock" };
    let my_pid = session_anchor_pid();
    let owner = if lock.pid == my_pid { "YOU" } else { "other process" };
    Some(format!("🔒 {} (pid {}, {}, {})", lock.id, lock.pid, owner, liveness))
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
        write_lock(&lp, 12345, "agent", "myagent").unwrap();
        let lock = read_lock(&lp).expect("lock should be readable after write");
        assert_eq!(lock.pid, 12345);
        assert_eq!(lock.kind, "agent");
        assert_eq!(lock.id, "myagent");
    }

    #[test]
    fn remove_lock_cleans_up() {
        let dir = TempDir::new().unwrap();
        let lp = dir.path().join("fp-lock");
        write_lock(&lp, 9999, "watch", "watch").unwrap();
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
        let lock = LockInfo { pid: std::process::id(), kind: "agent".into(), id: "test".into() };
        assert!(lock_is_live(&lock));
    }

    #[test]
    fn lock_is_live_with_dead_pid() {
        let lock = LockInfo { pid: 999_999_999, kind: "agent".into(), id: "test".into() };
        assert!(!lock_is_live(&lock));
    }

    #[test]
    fn repo_root_returns_main_root_from_worktree() {
        let base = TempDir::new().unwrap();
        let repo = base.path().join("myrepo");
        fs::create_dir_all(&repo).unwrap();
        std::process::Command::new("git").args(["init"]).current_dir(&repo).output().unwrap();
        std::process::Command::new("git").args(["config", "user.email", "t@t.com"]).current_dir(&repo).output().unwrap();
        std::process::Command::new("git").args(["config", "user.name", "T"]).current_dir(&repo).output().unwrap();
        fs::write(repo.join("f.txt"), "x").unwrap();
        std::process::Command::new("git").args(["add", "."]).current_dir(&repo).output().unwrap();
        std::process::Command::new("git").args(["commit", "-m", "init"]).current_dir(&repo).output().unwrap();
        std::process::Command::new("git").args(["branch", "feat"]).current_dir(&repo).output().unwrap();
        let wt = base.path().join("wt");
        std::process::Command::new("git").args(["worktree", "add", wt.to_str().unwrap(), "feat"]).current_dir(&repo).output().unwrap();

        let root_from_main = main_repo_root(&repo).expect("main_repo_root must succeed from main repo");
        let root_from_wt = main_repo_root(&wt).expect("main_repo_root must succeed from worktree");

        assert_eq!(
            fs::canonicalize(&root_from_main).unwrap(),
            fs::canonicalize(&root_from_wt).unwrap(),
            "repo_root must equal main repo root from worktree"
        );
    }

    #[test]
    fn main_repo_root_from_subdirectory_matches_root() {
        let base = TempDir::new().unwrap();
        let repo = base.path().join("myrepo");
        fs::create_dir_all(&repo).unwrap();
        std::process::Command::new("git").args(["init"]).current_dir(&repo).output().unwrap();
        std::process::Command::new("git").args(["config", "user.email", "t@t.com"]).current_dir(&repo).output().unwrap();
        std::process::Command::new("git").args(["config", "user.name", "T"]).current_dir(&repo).output().unwrap();
        fs::write(repo.join("f.txt"), "x").unwrap();
        std::process::Command::new("git").args(["add", "."]).current_dir(&repo).output().unwrap();
        std::process::Command::new("git").args(["commit", "-m", "init"]).current_dir(&repo).output().unwrap();
        let subdir = repo.join("subdir");
        fs::create_dir_all(&subdir).unwrap();

        let from_root = main_repo_root(&repo).expect("main_repo_root must succeed from root");
        let from_subdir = main_repo_root(&subdir).expect("main_repo_root must succeed from subdirectory");

        // Must return canonical paths (no ".." components) so file_name() works correctly
        assert_eq!(
            from_root,
            from_subdir,
            "main_repo_root from subdirectory must equal main_repo_root from root without canonicalize"
        );
        assert_ne!(
            from_subdir.file_name().unwrap_or_default(),
            "..",
            "main_repo_root must not return a path ending in '..'"
        );
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
    fn check_target_lock_errors_on_stale_lock() {
        let (_base, _repo, git_dir) = make_repo("myrepo");
        let lp = lock_path(&git_dir, "feat/auth");
        write_lock(&lp, 999_999_999, "agent", "old-session").unwrap();
        let result = check_target_lock(&git_dir, "feat/auth");
        assert!(result.is_err(), "stale lock must still block switch");
        assert!(lp.exists(), "stale lock file must NOT be auto-removed");
        assert!(result.unwrap_err().to_string().contains("locked"),
            "error must mention 'locked'");
    }

    #[test]
    fn check_target_lock_fails_when_live_lock() {
        let (_base, _repo, git_dir) = make_repo("myrepo");
        let lp = lock_path(&git_dir, "feat/auth");
        write_lock(&lp, std::process::id(), "agent", "live-session").unwrap();
        let result = check_target_lock(&git_dir, "feat/auth");
        assert!(result.is_err(), "live lock must block switch");
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("locked"), "error must mention 'locked'");
        // Guidance for agents: must explain own PID, stealing risk, and safety check
        assert!(msg.contains("Your anchor PID"), "error must tell agent its own PID");
        assert!(msg.contains("steal"), "error must explain stealing the lock");
        assert!(msg.contains("safe"), "error must explain how to tell it is safe");
    }

    #[test]
    fn check_target_lock_dead_lock_message_includes_guidance() {
        let (_base, _repo, git_dir) = make_repo("myrepo");
        let lp = lock_path(&git_dir, "feat/auth");
        write_lock(&lp, 999_999_999, "agent", "dead-session").unwrap();
        let result = check_target_lock(&git_dir, "feat/auth");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("dead"), "dead lock must be identified as dead");
        assert!(msg.contains("safe to steal"), "dead lock message must say safe to steal");
    }

    #[test]
    fn lock_status_none_when_no_lock() {
        let (_base, _repo, git_dir) = make_repo("myrepo");
        assert!(lock_status(&git_dir, "feat/auth").is_none());
    }

    #[test]
    fn lock_status_shows_id_and_alive_for_live_pid() {
        let (_base, _repo, git_dir) = make_repo("myrepo");
        let lp = lock_path(&git_dir, "feat/auth");
        let mut child = std::process::Command::new("sleep").arg("10").spawn().unwrap();
        let child_pid = child.id();
        write_lock(&lp, child_pid, "agent", "my-session").unwrap();
        let status = lock_status(&git_dir, "feat/auth").expect("lock status must be present");
        child.kill().ok();
        assert!(status.contains("my-session"), "lock status must show id, got: {}", status);
        assert!(status.contains(&format!("pid {}", child_pid)), "lock status must show pid, got: {}", status);
        assert!(status.contains("alive"), "lock status must say 'alive' for live pid, got: {}", status);
    }

    #[test]
    fn lock_status_shows_id_and_dead_for_dead_pid() {
        let (_base, _repo, git_dir) = make_repo("myrepo");
        let lp = lock_path(&git_dir, "feat/auth");
        write_lock(&lp, 999_999_999, "agent", "dead-session").unwrap();
        let status = lock_status(&git_dir, "feat/auth").expect("lock status must be present for dead lock");
        assert!(status.contains("dead-session"), "lock status must show id, got: {}", status);
        assert!(status.contains("dead"), "lock status must say 'dead' for dead pid, got: {}", status);
    }

    #[test]
    fn lock_status_none_when_no_lock_file() {
        let (_base, _repo, git_dir) = make_repo("myrepo");
        assert!(lock_status(&git_dir, "feat/auth").is_none());
    }

    #[test]
    fn parse_worktree_list_returns_path_for_checked_out_branch() {
        let output = "worktree /projects/main\nHEAD abc123\nbranch refs/heads/main\n\nworktree /projects/worktrees/feature/foo\nHEAD def456\nbranch refs/heads/feature/foo\n\n";
        let result = parse_worktree_list(output, "feature/foo");
        assert_eq!(result, Some(PathBuf::from("/projects/worktrees/feature/foo")),
            "parse_worktree_list must return path for checked-out branch");
    }

    #[test]
    fn parse_worktree_list_returns_none_for_absent_branch() {
        let output = "worktree /projects/main\nHEAD abc123\nbranch refs/heads/main\n\n";
        let result = parse_worktree_list(output, "feature/foo");
        assert!(result.is_none(), "parse_worktree_list must return None for absent branch");
    }

    #[test]
    fn session_anchor_pid_returns_live_non_self_pid() {
        let anchor = crate::worktree::session_anchor_pid();
        let self_pid = std::process::id();
        assert_ne!(anchor, self_pid,
            "session_anchor_pid() must not return self PID; got anchor={} self={}", anchor, self_pid);
        assert!(crate::worktree::pid_is_alive(anchor),
            "session_anchor_pid() must return a live process, got: {}", anchor);
    }
}
