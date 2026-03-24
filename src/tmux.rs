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
pub fn create_session(
    name: &str,
    dir: &str,
    prompt: Option<&str>,
    claude_args: Option<&str>,
    claude_session_id: &str,
    use_worktree: bool,
) -> Result<TmuxSession> {
    // Canonicalize the base directory so tmux gets an absolute, resolved path
    let abs_dir = std::path::Path::new(dir)
        .canonicalize()
        .map_err(|e| eyre!("Cannot resolve directory '{dir}': {e}"))?
        .to_string_lossy()
        .to_string();

    // Build the claude command with optional extra args and prompt
    let mut claude_parts = if use_worktree {
        format!("claude --worktree {name} --session-id {claude_session_id}")
    } else {
        format!("claude --session-id {claude_session_id}")
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

    // Create detached session with claude window, starting in the project directory
    let output = Command::new("tmux")
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

/// Create the editor window + split pane for an existing tmux session.
/// Called when Claude's SessionStart hook fires, so the worktree is guaranteed to exist.
pub fn setup_editor_window(session_name: &str, directory: &str) -> Result<()> {
    let abs_dir = std::path::Path::new(directory)
        .canonicalize()
        .map_err(|e| eyre!("Cannot resolve directory '{directory}': {e}"))?
        .to_string_lossy()
        .to_string();

    let worktree_dir = format!("{abs_dir}/.claude/worktrees/{session_name}");
    let worktree_path = std::path::Path::new(&worktree_dir);

    // Use worktree dir if it exists, otherwise fall back to project dir
    let editor_dir = if worktree_path.exists() {
        &worktree_dir
    } else {
        &abs_dir
    };

    // Split claude window horizontally to add a terminal pane
    run_tmux(&[
        "split-window",
        "-h",
        "-t",
        &format!("{session_name}:claude"),
        "-c",
        editor_dir,
    ])?;

    // Create editor window with vim
    run_tmux(&[
        "new-window",
        "-t",
        session_name,
        "-n",
        "editor",
        "-c",
        editor_dir,
    ])?;

    // Open vim in editor window
    run_tmux(&[
        "send-keys",
        "-t",
        &format!("{session_name}:editor"),
        "vim .",
        "Enter",
    ])?;

    // Split editor window horizontally
    run_tmux(&[
        "split-window",
        "-h",
        "-t",
        &format!("{session_name}:editor"),
        "-c",
        editor_dir,
    ])?;

    // Select the claude window and focus the claude pane (left)
    run_tmux(&["select-window", "-t", &format!("{session_name}:claude")])?;
    run_tmux(&["select-pane", "-t", &format!("{session_name}:claude.0")])?;

    Ok(())
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

        let result = create_session(name, dir, None, None, "test-uuid-123", true);
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
