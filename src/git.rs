use color_eyre::{Result, eyre::eyre};
use std::process::Command;

/// Prepare a branch in the given directory: stash if dirty, create/checkout branch, sync with origin.
pub fn prepare_branch(directory: &str, branch_name: &str) -> Result<()> {
    stash_if_dirty(directory, &format!("switching to {branch_name}"))?;

    // Check if the branch exists locally
    let local_exists = branch_exists_locally(directory, branch_name)?;

    if local_exists {
        run_git(directory, &["checkout", branch_name])?;
    } else {
        run_git(directory, &["checkout", "-b", branch_name])?;
    }

    sync_with_origin(directory, branch_name);

    Ok(())
}

/// Prepare the repo for worktree creation: stash if dirty, checkout main, and pull latest.
pub fn prepare_worktree(directory: &str) -> Result<()> {
    let main_branch = detect_main_branch(directory)?;

    stash_if_dirty(
        directory,
        &format!("worktree creation (switching to {main_branch})"),
    )?;

    run_git(directory, &["checkout", &main_branch])?;

    sync_with_origin(directory, &main_branch);

    Ok(())
}

/// Stash uncommitted changes if the working tree is dirty.
fn stash_if_dirty(directory: &str, context: &str) -> Result<()> {
    if has_uncommitted_changes(directory)? {
        let output = Command::new("git")
            .args([
                "stash",
                "push",
                "-m",
                &format!("van-damme: auto-stash before {context}"),
            ])
            .current_dir(directory)
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(eyre!("Failed to stash changes: {stderr}"));
        }
    }
    Ok(())
}

/// Fetch from origin and fast-forward merge. Best-effort — won't fail if remote doesn't exist.
fn sync_with_origin(directory: &str, branch_name: &str) {
    let fetch_result = Command::new("git")
        .args(["fetch", "origin", branch_name])
        .current_dir(directory)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    if let Ok(status) = fetch_result
        && status.success()
    {
        let remote_ref = format!("origin/{branch_name}");
        let merge_output = Command::new("git")
            .args(["merge", "--ff-only", &remote_ref])
            .current_dir(directory)
            .output();
        if let Ok(output) = merge_output
            && !output.status.success()
        {
            // ff-only failed (diverged history) — try rebase instead
            let _ = Command::new("git")
                .args(["rebase", &remote_ref])
                .current_dir(directory)
                .output();
        }
    }
}

/// Detect the main branch name for a repository.
/// Tries: origin/HEAD symbolic ref → "main" exists → "master" exists.
fn detect_main_branch(directory: &str) -> Result<String> {
    // Try symbolic-ref of origin/HEAD
    let output = Command::new("git")
        .args(["symbolic-ref", "refs/remotes/origin/HEAD"])
        .current_dir(directory)
        .output();

    if let Ok(out) = output
        && out.status.success()
    {
        let refpath = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if let Some(branch) = refpath.rsplit('/').next()
            && !branch.is_empty()
        {
            return Ok(branch.to_string());
        }
    }

    // Fallback: check for "main" branch locally
    if branch_exists_locally(directory, "main")? {
        return Ok("main".to_string());
    }

    // Fallback: check for "master" branch locally
    if branch_exists_locally(directory, "master")? {
        return Ok("master".to_string());
    }

    Err(eyre!(
        "Could not detect main branch: no origin/HEAD, 'main', or 'master' branch found"
    ))
}

fn has_uncommitted_changes(directory: &str) -> Result<bool> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(directory)
        .output()?;
    if !output.status.success() {
        return Err(eyre!("Failed to check git status"));
    }
    Ok(!output.stdout.is_empty())
}

fn branch_exists_locally(directory: &str, branch_name: &str) -> Result<bool> {
    let output = Command::new("git")
        .args(["rev-parse", "--verify", branch_name])
        .current_dir(directory)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()?;
    Ok(output.success())
}

fn run_git(directory: &str, args: &[&str]) -> Result<()> {
    let output = Command::new("git")
        .args(args)
        .current_dir(directory)
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(eyre!("git {} failed: {}", args.join(" "), stderr.trim()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn init_test_repo(dir: &std::path::Path) {
        Command::new("git")
            .args(["init"])
            .current_dir(dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(dir)
            .output()
            .unwrap();
        // Create initial commit so we have a branch to work with
        std::fs::write(dir.join("README.md"), "init").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(dir)
            .output()
            .unwrap();
        // Ensure the default branch is called "main" regardless of git config
        Command::new("git")
            .args(["branch", "-M", "main"])
            .current_dir(dir)
            .output()
            .unwrap();
    }

    #[test]
    fn test_has_uncommitted_changes_clean() {
        let tmp = tempfile::tempdir().unwrap();
        init_test_repo(tmp.path());
        let result = has_uncommitted_changes(tmp.path().to_str().unwrap()).unwrap();
        assert!(!result);
    }

    #[test]
    fn test_has_uncommitted_changes_dirty() {
        let tmp = tempfile::tempdir().unwrap();
        init_test_repo(tmp.path());
        std::fs::write(tmp.path().join("dirty.txt"), "dirty").unwrap();
        let result = has_uncommitted_changes(tmp.path().to_str().unwrap()).unwrap();
        assert!(result);
    }

    #[test]
    fn test_branch_exists_locally_false() {
        let tmp = tempfile::tempdir().unwrap();
        init_test_repo(tmp.path());
        let result = branch_exists_locally(tmp.path().to_str().unwrap(), "nonexistent").unwrap();
        assert!(!result);
    }

    #[test]
    fn test_branch_exists_locally_true() {
        let tmp = tempfile::tempdir().unwrap();
        init_test_repo(tmp.path());
        Command::new("git")
            .args(["branch", "feature-x"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        let result = branch_exists_locally(tmp.path().to_str().unwrap(), "feature-x").unwrap();
        assert!(result);
    }

    #[test]
    fn test_prepare_branch_creates_new_branch() {
        let tmp = tempfile::tempdir().unwrap();
        init_test_repo(tmp.path());
        let dir = tmp.path().to_str().unwrap();

        prepare_branch(dir, "my-feature").unwrap();

        // Verify we're on the new branch
        let output = Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
        assert_eq!(branch, "my-feature");
    }

    #[test]
    fn test_prepare_branch_stashes_changes() {
        let tmp = tempfile::tempdir().unwrap();
        init_test_repo(tmp.path());
        let dir = tmp.path().to_str().unwrap();

        // Modify an already-tracked file to create a dirty working tree
        std::fs::write(tmp.path().join("README.md"), "modified").unwrap();
        assert!(has_uncommitted_changes(dir).unwrap());

        prepare_branch(dir, "my-feature").unwrap();

        // Changes should be stashed (clean working tree)
        assert!(!has_uncommitted_changes(dir).unwrap());

        // Verify stash exists
        let output = Command::new("git")
            .args(["stash", "list"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        let stash_list = String::from_utf8_lossy(&output.stdout);
        assert!(stash_list.contains("van-damme: auto-stash"));
    }

    #[test]
    fn test_prepare_branch_checks_out_existing_branch() {
        let tmp = tempfile::tempdir().unwrap();
        init_test_repo(tmp.path());
        let dir = tmp.path().to_str().unwrap();

        // Create the branch first
        Command::new("git")
            .args(["branch", "existing-branch"])
            .current_dir(tmp.path())
            .output()
            .unwrap();

        prepare_branch(dir, "existing-branch").unwrap();

        let output = Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
        assert_eq!(branch, "existing-branch");
    }

    fn current_branch(dir: &std::path::Path) -> String {
        let output = Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(dir)
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    #[test]
    fn test_detect_main_branch_with_main() {
        let tmp = tempfile::tempdir().unwrap();
        init_test_repo(tmp.path());
        let dir = tmp.path().to_str().unwrap();

        let result = detect_main_branch(dir).unwrap();
        assert_eq!(result, "main");
    }

    #[test]
    fn test_detect_main_branch_with_master() {
        let tmp = tempfile::tempdir().unwrap();
        init_test_repo(tmp.path());
        let dir = tmp.path().to_str().unwrap();

        // Rename "main" to "master"
        Command::new("git")
            .args(["branch", "-M", "main", "master"])
            .current_dir(tmp.path())
            .output()
            .unwrap();

        let result = detect_main_branch(dir).unwrap();
        assert_eq!(result, "master");
    }

    #[test]
    fn test_detect_main_branch_no_main_or_master() {
        let tmp = tempfile::tempdir().unwrap();
        init_test_repo(tmp.path());
        let dir = tmp.path().to_str().unwrap();

        // Rename to something that is neither main nor master
        Command::new("git")
            .args(["branch", "-M", "main", "develop"])
            .current_dir(tmp.path())
            .output()
            .unwrap();

        let result = detect_main_branch(dir);
        assert!(result.is_err());
    }

    #[test]
    fn test_prepare_worktree_clean_repo() {
        let tmp = tempfile::tempdir().unwrap();
        init_test_repo(tmp.path());
        let dir = tmp.path().to_str().unwrap();

        // Already on main, clean — should be a no-op
        prepare_worktree(dir).unwrap();

        assert_eq!(current_branch(tmp.path()), "main");
        assert!(!has_uncommitted_changes(dir).unwrap());
    }

    #[test]
    fn test_prepare_worktree_checks_out_main() {
        let tmp = tempfile::tempdir().unwrap();
        init_test_repo(tmp.path());
        let dir = tmp.path().to_str().unwrap();

        // Switch to a feature branch
        Command::new("git")
            .args(["checkout", "-b", "feature-x"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        assert_eq!(current_branch(tmp.path()), "feature-x");

        prepare_worktree(dir).unwrap();

        assert_eq!(current_branch(tmp.path()), "main");
    }

    #[test]
    fn test_prepare_worktree_stashes_changes() {
        let tmp = tempfile::tempdir().unwrap();
        init_test_repo(tmp.path());
        let dir = tmp.path().to_str().unwrap();

        // Create dirty working tree
        std::fs::write(tmp.path().join("README.md"), "modified").unwrap();
        assert!(has_uncommitted_changes(dir).unwrap());

        prepare_worktree(dir).unwrap();

        // Changes should be stashed
        assert!(!has_uncommitted_changes(dir).unwrap());

        let output = Command::new("git")
            .args(["stash", "list"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        let stash_list = String::from_utf8_lossy(&output.stdout);
        assert!(stash_list.contains("van-damme: auto-stash"));
    }

    #[test]
    fn test_prepare_worktree_feature_branch_dirty() {
        let tmp = tempfile::tempdir().unwrap();
        init_test_repo(tmp.path());
        let dir = tmp.path().to_str().unwrap();

        // Switch to feature branch and make it dirty
        Command::new("git")
            .args(["checkout", "-b", "feature-y"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        std::fs::write(tmp.path().join("README.md"), "dirty on feature").unwrap();
        assert_eq!(current_branch(tmp.path()), "feature-y");
        assert!(has_uncommitted_changes(dir).unwrap());

        prepare_worktree(dir).unwrap();

        // Should be on main with clean working tree
        assert_eq!(current_branch(tmp.path()), "main");
        assert!(!has_uncommitted_changes(dir).unwrap());

        // Stash should exist
        let output = Command::new("git")
            .args(["stash", "list"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        let stash_list = String::from_utf8_lossy(&output.stdout);
        assert!(stash_list.contains("van-damme: auto-stash"));
    }
}
