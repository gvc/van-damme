use color_eyre::{Result, eyre::eyre};
use std::process::Command;

/// Prepare a branch in the given directory: stash if dirty, create/checkout branch, sync with origin.
pub fn prepare_branch(directory: &str, branch_name: &str) -> Result<()> {
    // Stash any unstaged/uncommitted changes
    let has_changes = has_uncommitted_changes(directory)?;
    if has_changes {
        let status = Command::new("git")
            .args([
                "stash",
                "push",
                "-m",
                &format!("van-damme: auto-stash before switching to {branch_name}"),
            ])
            .current_dir(directory)
            .output()?;
        if !status.status.success() {
            let stderr = String::from_utf8_lossy(&status.stderr);
            return Err(eyre!("Failed to stash changes: {stderr}"));
        }
    }

    // Check if the branch exists locally
    let local_exists = branch_exists_locally(directory, branch_name)?;

    if local_exists {
        // Checkout the existing branch
        run_git(directory, &["checkout", branch_name])?;
    } else {
        // Create and checkout a new branch
        run_git(directory, &["checkout", "-b", branch_name])?;
    }

    // Try to sync with origin (fetch + pull), but don't fail if remote doesn't exist
    let fetch_result = Command::new("git")
        .args(["fetch", "origin", branch_name])
        .current_dir(directory)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    if let Ok(status) = fetch_result
        && status.success()
    {
        // Remote branch exists, pull changes
        let pull_output = Command::new("git")
            .args(["pull", "origin", branch_name, "--ff-only"])
            .current_dir(directory)
            .output()?;
        if !pull_output.status.success() {
            // Non-fast-forward — try rebase instead
            let _ = Command::new("git")
                .args(["pull", "origin", branch_name, "--rebase"])
                .current_dir(directory)
                .output();
        }
    }

    Ok(())
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
}
