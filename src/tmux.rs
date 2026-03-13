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
/// If `prompt` is provided, it will be sent to Claude as an initial prompt.
/// If `claude_args` is provided, they are inserted before the prompt on the CLI.
pub fn create_session(
    name: &str,
    dir: &str,
    prompt: Option<&str>,
    claude_args: Option<&str>,
) -> Result<TmuxSession> {
    // Canonicalize the base directory so tmux gets an absolute, resolved path
    let abs_dir = std::path::Path::new(dir)
        .canonicalize()
        .map_err(|e| eyre!("Cannot resolve directory '{dir}': {e}"))?
        .to_string_lossy()
        .to_string();

    // The worktree lives at <dir>/.claude/worktrees/<name>
    // Don't create it manually — Claude Code's --worktree flag handles that via git worktree add
    let worktree_dir = format!("{abs_dir}/.claude/worktrees/{name}");

    // Build the claude command with optional extra args and prompt
    let mut claude_cmd = format!("claude --worktree {name}");
    if let Some(args) = claude_args {
        claude_cmd.push(' ');
        claude_cmd.push_str(args);
    }
    if let Some(p) = prompt {
        claude_cmd.push(' ');
        claude_cmd.push_str(&shell_escape(p));
    }

    // Create detached session with claude window, starting in the project directory
    let status = Command::new("tmux")
        .args([
            "new-session",
            "-d",
            "-s",
            name,
            "-n",
            "claude",
            "-c",
            &abs_dir,
            &claude_cmd,
        ])
        .status()?;
    if !status.success() {
        return Err(eyre!("Failed to create tmux session '{name}'"));
    }

    // Wait for Claude Code to create the git worktree before opening the editor window
    let worktree_path = std::path::Path::new(&worktree_dir);
    let max_wait = std::time::Duration::from_secs(10);
    let poll_interval = std::time::Duration::from_millis(250);
    let start = std::time::Instant::now();
    while !worktree_path.join(".git").exists() {
        if start.elapsed() >= max_wait {
            // Fall back to project dir if worktree wasn't created in time
            break;
        }
        std::thread::sleep(poll_interval);
    }

    // Use worktree dir if it exists, otherwise fall back to project dir
    let editor_dir = if worktree_path.exists() {
        &worktree_dir
    } else {
        &abs_dir
    };

    // Create editor window
    run_tmux(&["new-window", "-t", name, "-n", "editor", "-c", editor_dir])?;

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
        editor_dir,
    ])?;

    // Select the first window (claude) as the default when attaching
    run_tmux(&["select-window", "-t", &format!("{name}:claude")])?;

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

/// Remove the worktree directory for a session.
/// The worktree lives at `<dir>/.claude/worktrees/<name>`.
pub fn remove_worktree(dir: &str, name: &str) -> Result<()> {
    let worktree_dir = std::path::PathBuf::from(dir)
        .join(".claude")
        .join("worktrees")
        .join(name);
    if worktree_dir.exists() {
        std::fs::remove_dir_all(&worktree_dir)?;
    }
    Ok(())
}

/// Escape a string for safe use as a single shell argument.
fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
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
    fn test_shell_escape_simple() {
        assert_eq!(shell_escape("hello world"), "'hello world'");
    }

    #[test]
    fn test_shell_escape_with_quotes() {
        assert_eq!(shell_escape("it's a test"), "'it'\\''s a test'");
    }

    #[test]
    fn test_shell_escape_empty() {
        assert_eq!(shell_escape(""), "''");
    }

    #[test]
    fn test_remove_worktree_removes_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_str().unwrap();
        let name = "test-session";

        // Create the worktree directory
        let worktree = tmp.path().join(".claude").join("worktrees").join(name);
        std::fs::create_dir_all(&worktree).unwrap();
        std::fs::write(worktree.join("file.txt"), "hello").unwrap();
        assert!(worktree.exists());

        remove_worktree(dir, name).unwrap();
        assert!(!worktree.exists());
    }

    #[test]
    fn test_remove_worktree_nonexistent_is_ok() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_str().unwrap();
        // Should not error when the directory doesn't exist
        assert!(remove_worktree(dir, "nonexistent").is_ok());
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

        let result = create_session(name, dir, None, None);
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
