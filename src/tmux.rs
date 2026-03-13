use color_eyre::{Result, eyre::eyre};
use std::process::Command;

#[derive(Debug)]
pub struct TmuxSession {
    pub session_name: String,
    pub session_id: String,
}

/// Sanitize a task title into a valid tmux session name.
/// Lowercases, replaces whitespace runs with hyphens, strips non-alphanumeric/non-hyphen chars,
/// and trims leading/trailing hyphens.
pub fn sanitize_session_name(title: &str) -> String {
    let lowered = title.to_lowercase();
    let mut result = String::new();
    let mut prev_was_sep = true; // start true to trim leading hyphens

    for ch in lowered.chars() {
        if ch.is_whitespace() || ch == '-' {
            if !prev_was_sep {
                result.push('-');
                prev_was_sep = true;
            }
        } else if ch.is_alphanumeric() {
            result.push(ch);
            prev_was_sep = false;
        }
        // strip everything else
    }

    // Trim trailing hyphen
    while result.ends_with('-') {
        result.pop();
    }

    result
}

/// Check if a tmux session with the given name already exists.
pub fn session_exists(name: &str) -> Result<bool> {
    let status = Command::new("tmux")
        .args(["has-session", "-t", name])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()?;
    Ok(status.success())
}

/// Create a new tmux session with Claude and editor windows.
pub fn create_session(name: &str, dir: &str) -> Result<TmuxSession> {
    // Create detached session with claude window
    let status = Command::new("tmux")
        .args([
            "new-session",
            "-d",
            "-s",
            name,
            "-n",
            "claude",
            "-c",
            dir,
            &format!("claude --worktree {name}"),
        ])
        .status()?;
    if !status.success() {
        return Err(eyre!("Failed to create tmux session '{name}'"));
    }

    // The worktree lives at <dir>/.claude/<name>
    let worktree_dir = format!("{dir}/.claude/{name}");

    // Ensure the worktree directory exists before opening editor in it
    // (claude --worktree creates it, but there may be a race)
    std::fs::create_dir_all(&worktree_dir).ok();

    // Create editor window in the worktree directory
    run_tmux(&[
        "new-window",
        "-t",
        name,
        "-n",
        "editor",
        "-c",
        &worktree_dir,
    ])?;

    // Open vim in editor window
    run_tmux(&[
        "send-keys",
        "-t",
        &format!("{name}:editor"),
        "vim .",
        "Enter",
    ])?;

    // Split editor window horizontally
    run_tmux(&[
        "split-window",
        "-h",
        "-t",
        &format!("{name}:editor"),
        "-c",
        &worktree_dir,
    ])?;

    // Capture session ID
    let output = Command::new("tmux")
        .args(["display-message", "-t", name, "-p", "#{session_id}"])
        .output()?;
    if !output.status.success() {
        return Err(eyre!("Failed to get session ID for '{name}'"));
    }

    let session_id = String::from_utf8_lossy(&output.stdout).trim().to_string();

    Ok(TmuxSession {
        session_name: name.to_string(),
        session_id,
    })
}

/// Kill a tmux session by name.
pub fn kill_session(name: &str) -> Result<()> {
    run_tmux(&["kill-session", "-t", name])
}

fn run_tmux(args: &[&str]) -> Result<()> {
    let status = Command::new("tmux").args(args).status()?;
    if !status.success() {
        return Err(eyre!("tmux command failed: tmux {}", args.join(" ")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_basic() {
        assert_eq!(sanitize_session_name("My Task"), "my-task");
    }

    #[test]
    fn test_sanitize_multiple_spaces() {
        assert_eq!(sanitize_session_name("hello   world"), "hello-world");
    }

    #[test]
    fn test_sanitize_special_chars() {
        assert_eq!(sanitize_session_name("task@#$name!"), "taskname");
    }

    #[test]
    fn test_sanitize_leading_trailing_spaces() {
        assert_eq!(sanitize_session_name("  hello  "), "hello");
    }

    #[test]
    fn test_sanitize_hyphens_preserved() {
        assert_eq!(sanitize_session_name("my-task-name"), "my-task-name");
    }

    #[test]
    fn test_sanitize_mixed_separators() {
        assert_eq!(sanitize_session_name("my - task - name"), "my-task-name");
    }

    #[test]
    fn test_sanitize_empty() {
        assert_eq!(sanitize_session_name(""), "");
    }

    #[test]
    fn test_sanitize_only_special_chars() {
        assert_eq!(sanitize_session_name("@#$%"), "");
    }

    #[test]
    fn test_sanitize_numbers() {
        assert_eq!(sanitize_session_name("task 123"), "task-123");
    }

    #[test]
    fn test_sanitize_unicode() {
        assert_eq!(sanitize_session_name("café résumé"), "café-résumé");
    }

    #[test]
    #[ignore] // Requires tmux to be running
    fn test_session_exists_nonexistent() {
        let result = session_exists("nonexistent-session-12345");
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[test]
    #[ignore] // Requires tmux to be running
    fn test_create_and_cleanup_session() {
        let name = "van-damme-test-session";
        let dir = "/tmp";

        // Clean up if exists from a previous failed run
        let _ = Command::new("tmux")
            .args(["kill-session", "-t", name])
            .status();

        let result = create_session(name, dir);
        assert!(result.is_ok());

        let session = result.unwrap();
        assert_eq!(session.session_name, name);
        assert!(!session.session_id.is_empty());

        // Verify it exists
        assert!(session_exists(name).unwrap());

        // Clean up
        let _ = Command::new("tmux")
            .args(["kill-session", "-t", name])
            .status();
    }
}
