use color_eyre::{Result, eyre::eyre};
use std::process::Command;

#[derive(Debug)]
pub struct TmuxSession {
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
/// The `claude_session_id` is the pre-generated UUID used for `--session-id`.
/// If `use_worktree` is true, Claude is launched with `--worktree <name>`.
/// `claude_command` is the executable name/path to invoke (default: "claude").
pub fn create_session(
    name: &str,
    dir: &str,
    prompt: Option<&str>,
    claude_args: Option<&str>,
    claude_session_id: &str,
    use_worktree: bool,
    claude_command: &str,
) -> Result<TmuxSession> {
    // Canonicalize the base directory so tmux gets an absolute, resolved path
    let abs_dir = std::path::Path::new(dir)
        .canonicalize()
        .map_err(|e| eyre!("Cannot resolve directory '{dir}': {e}"))?
        .to_string_lossy()
        .to_string();

    // Build the claude command with optional extra args and prompt
    let mut claude_parts = if use_worktree {
        format!("{claude_command} --worktree {name} --session-id {claude_session_id}")
    } else {
        format!("{claude_command} --session-id {claude_session_id}")
    };
    if let Some(args) = claude_args {
        claude_parts.push(' ');
        claude_parts.push_str(args);
    }
    if let Some(p) = prompt {
        claude_parts.push(' ');
        claude_parts.push_str(&shell_escape(p));
    }
    // Wrap so that if claude exits with an error, the pane stays open showing it
    let claude_cmd = format!(
        "{claude_parts} || {{ echo ''; echo '[van-damme] claude exited with code '$?; read; }}"
    );

    let window_name = window_name_from_command(claude_command);

    // Create detached session with claude window, starting in the project directory
    let output = Command::new("tmux")
        .args([
            "new-session",
            "-d",
            "-s",
            name,
            "-n",
            window_name,
            "-c",
            &abs_dir,
            &claude_cmd,
        ])
        .stdin(std::process::Stdio::null())
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(eyre!("Failed to create tmux session '{name}': {stderr}"));
    }

    // Capture session ID
    let output = Command::new("tmux")
        .args(["display-message", "-t", name, "-p", "#{session_id}"])
        .output()?;
    if !output.status.success() {
        return Err(eyre!("Failed to get session ID for '{name}'"));
    }

    let session_id = String::from_utf8_lossy(&output.stdout).trim().to_string();

    Ok(TmuxSession { session_id })
}

/// Add a terminal split pane to the right of Claude in an existing tmux session.
/// Called when Claude's SessionStart hook fires, so the worktree is guaranteed to exist.
/// `window_name` is the tmux window name (e.g. "claude", "cc") — must match what was
/// used when creating the session.
pub fn setup_editor_window(session_name: &str, directory: &str, window_name: &str) -> Result<()> {
    let abs_dir = std::path::Path::new(directory)
        .canonicalize()
        .map_err(|e| eyre!("Cannot resolve directory '{directory}': {e}"))?
        .to_string_lossy()
        .to_string();

    let worktree_dir = format!("{abs_dir}/.claude/worktrees/{session_name}");
    let worktree_path = std::path::Path::new(&worktree_dir);

    // Use worktree dir if it exists, otherwise fall back to project dir
    let pane_dir = if worktree_path.exists() {
        &worktree_dir
    } else {
        &abs_dir
    };

    let target = format!("{session_name}:{window_name}");

    // Split claude window horizontally to add a terminal pane on the right
    run_tmux(&["split-window", "-h", "-t", &target, "-c", pane_dir])?;

    // Split the right pane vertically to get two stacked panes on the right
    run_tmux(&["split-window", "-v", "-t", &target, "-c", pane_dir])?;

    // Focus back on the claude pane (left)
    run_tmux(&["select-pane", "-L", "-t", &target])?;

    Ok(())
}

/// Create a plain tmux session (no Claude) with a single window and a vertical split.
/// Both panes start in the given directory.
pub fn create_plain_session(name: &str, dir: &str) -> Result<TmuxSession> {
    let abs_dir = std::path::Path::new(dir)
        .canonicalize()
        .map_err(|e| eyre!("Cannot resolve directory '{dir}': {e}"))?
        .to_string_lossy()
        .to_string();

    // Create detached session starting in the directory
    let output = Command::new("tmux")
        .args(["new-session", "-d", "-s", name, "-c", &abs_dir])
        .stdin(std::process::Stdio::null())
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(eyre!("Failed to create tmux session '{name}': {stderr}"));
    }

    // Split the window vertically
    run_tmux(&["split-window", "-h", "-t", name, "-c", &abs_dir])?;

    // Focus the left pane
    run_tmux(&["select-pane", "-L", "-t", name])?;

    // Capture session ID
    let output = Command::new("tmux")
        .args(["display-message", "-t", name, "-p", "#{session_id}"])
        .output()?;
    if !output.status.success() {
        return Err(eyre!("Failed to get session ID for '{name}'"));
    }

    let session_id = String::from_utf8_lossy(&output.stdout).trim().to_string();

    Ok(TmuxSession { session_id })
}

/// Switch the current tmux client to the given session.
/// Use this instead of attach-session when already inside tmux.
pub fn switch_to_session(name: &str) -> Result<()> {
    run_tmux(&["switch-client", "-t", name])
}

/// Kill a tmux session by name.
pub fn kill_session(name: &str) -> Result<()> {
    run_tmux(&["kill-session", "-t", name])
}

/// Extract the base command name from a claude_command string to use as a tmux window name.
/// e.g. "/usr/bin/claude" -> "claude", "cc" -> "cc", "claude" -> "claude"
pub fn window_name_from_command(claude_command: &str) -> &str {
    std::path::Path::new(claude_command)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(claude_command)
}

/// Escape a string for safe use as a single shell argument.
fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn run_tmux(args: &[&str]) -> Result<()> {
    let output = Command::new("tmux").args(args).output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(eyre!(
            "tmux command failed: tmux {} — {}",
            args.join(" "),
            stderr.trim()
        ));
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
    fn test_window_name_from_command_simple() {
        assert_eq!(window_name_from_command("claude"), "claude");
    }

    #[test]
    fn test_window_name_from_command_path() {
        assert_eq!(window_name_from_command("/usr/bin/claude"), "claude");
    }

    #[test]
    fn test_window_name_from_command_short() {
        assert_eq!(window_name_from_command("cc"), "cc");
    }

    #[test]
    fn test_window_name_from_command_relative_path() {
        assert_eq!(window_name_from_command("./bin/my-claude"), "my-claude");
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

        let result = create_session(name, dir, None, None, "test-uuid-123", true, "claude");
        assert!(result.is_ok());

        let session = result.unwrap();
        assert!(!session.session_id.is_empty());

        // Verify it exists
        assert!(session_exists(name).unwrap());

        // Clean up
        let _ = Command::new("tmux")
            .args(["kill-session", "-t", name])
            .status();
    }
}
