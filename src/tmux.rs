use color_eyre::{Result, eyre::eyre};
use std::process::Command;

#[derive(Debug)]
pub struct TmuxSession {
    pub session_id: String,
}

#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub window_count: usize,
    pub pane_count: usize,
    /// Non-shell programs running in panes (deduped, sorted).
    pub programs: Vec<String>,
}

const SHELL_NAMES: &[&str] = &["zsh", "bash", "sh", "fish", "dash", "tcsh", "csh"];

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
#[allow(clippy::too_many_arguments)]
pub fn create_session(
    name: &str,
    dir: &str,
    prompt: Option<&str>,
    claude_args: Option<&str>,
    claude_session_id: &str,
    use_worktree: bool,
    claude_command: &str,
    model_id: Option<&str>,
) -> Result<TmuxSession> {
    // Canonicalize the base directory so tmux gets an absolute, resolved path
    let abs_dir = std::path::Path::new(dir)
        .canonicalize()
        .map_err(|e| eyre!("Cannot resolve directory '{dir}': {e}"))?
        .to_string_lossy()
        .to_string();

    // Build the claude command with optional extra args and prompt
    let claude_cmd = build_claude_cmd(
        name,
        claude_session_id,
        use_worktree,
        claude_command,
        model_id,
        claude_args,
        prompt,
    );

    let window_name = window_name_from_command(claude_command);

    // Create detached session with an idle shell — pane survives if claude exits
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
        ])
        .stdin(std::process::Stdio::null())
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(eyre!("Failed to create tmux session '{name}': {stderr}"));
    }

    // Send the claude command to the pane so it starts in the shell context.
    // The pane stays alive after claude exits (shell remains).
    let target = format!("{name}:{window_name}");
    let send_output = Command::new("tmux")
        .args(["send-keys", "-t", &target, &claude_cmd, "Enter"])
        .output()?;
    if !send_output.status.success() {
        let stderr = String::from_utf8_lossy(&send_output.stderr);
        return Err(eyre!(
            "Failed to send claude command to pane '{target}': {stderr}"
        ));
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

/// Capture the visible content of the active pane in a tmux session.
pub fn capture_pane(session_name: &str) -> Result<String> {
    let output = Command::new("tmux")
        .args(["capture-pane", "-p", "-t", session_name])
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(eyre!(
            "Failed to capture pane for '{session_name}': {stderr}"
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Return a summary of windows, panes, and running programs for a tmux session.
pub fn session_summary(session_name: &str) -> Result<SessionSummary> {
    let output = Command::new("tmux")
        .args([
            "list-panes",
            "-s",
            "-t",
            session_name,
            "-F",
            "#{window_index}:#{pane_current_command}",
        ])
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(eyre!("Failed to list panes for '{session_name}': {stderr}"));
    }
    Ok(parse_summary(&String::from_utf8_lossy(&output.stdout)))
}

/// Parse `tmux list-panes -F '#{window_index}:#{pane_current_command}'` output.
pub fn parse_summary(stdout: &str) -> SessionSummary {
    use std::collections::HashSet;

    let mut window_indices: HashSet<&str> = HashSet::new();
    let mut pane_count = 0usize;
    let mut program_set: HashSet<String> = HashSet::new();

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        pane_count += 1;
        if let Some((win_idx, cmd)) = line.split_once(':') {
            window_indices.insert(win_idx);
            if !SHELL_NAMES.contains(&cmd) && !cmd.is_empty() {
                program_set.insert(cmd.to_string());
            }
        }
    }

    let mut programs: Vec<String> = program_set.into_iter().collect();
    programs.sort();

    SessionSummary {
        window_count: window_indices.len(),
        pane_count,
        programs,
    }
}

/// Extract the base command name from a claude_command string to use as a tmux window name.
/// e.g. "/usr/bin/claude" -> "claude", "cc" -> "cc", "claude" -> "claude"
pub fn window_name_from_command(claude_command: &str) -> &str {
    std::path::Path::new(claude_command)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(claude_command)
}

/// Build the full claude command string sent to the tmux pane.
fn build_claude_cmd(
    name: &str,
    claude_session_id: &str,
    use_worktree: bool,
    claude_command: &str,
    model_id: Option<&str>,
    claude_args: Option<&str>,
    prompt: Option<&str>,
) -> String {
    let mut parts = if use_worktree {
        format!("{claude_command} --worktree {name} --session-id {claude_session_id}")
    } else {
        format!("{claude_command} --session-id {claude_session_id}")
    };
    if let Some(model) = model_id {
        parts.push_str(&format!(" --model {model}"));
    }
    if let Some(args) = claude_args {
        parts.push(' ');
        parts.push_str(args);
    }
    if let Some(p) = prompt {
        parts.push(' ');
        parts.push_str(&shell_escape(p));
    }
    format!("{parts} || {{ echo ''; echo '[van-damme] claude exited with code '$?; read; }}")
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
    fn test_parse_summary_counts_windows_and_panes() {
        let input = "0:claude\n0:zsh\n1:nvim\n1:zsh\n";
        let s = parse_summary(input);
        assert_eq!(s.window_count, 2);
        assert_eq!(s.pane_count, 4);
        assert_eq!(s.programs, vec!["claude", "nvim"]);
    }

    #[test]
    fn test_parse_summary_filters_shells() {
        let input = "0:zsh\n0:bash\n0:fish\n";
        let s = parse_summary(input);
        assert_eq!(s.pane_count, 3);
        assert!(s.programs.is_empty());
    }

    #[test]
    fn test_parse_summary_dedupes_programs() {
        let input = "0:claude\n1:claude\n2:claude\n";
        let s = parse_summary(input);
        assert_eq!(s.programs, vec!["claude"]);
    }

    #[test]
    fn test_parse_summary_empty() {
        let s = parse_summary("");
        assert_eq!(s.window_count, 0);
        assert_eq!(s.pane_count, 0);
        assert!(s.programs.is_empty());
    }

    #[test]
    fn test_parse_summary_single_pane() {
        let input = "0:claude\n";
        let s = parse_summary(input);
        assert_eq!(s.window_count, 1);
        assert_eq!(s.pane_count, 1);
        assert_eq!(s.programs, vec!["claude"]);
    }

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
    fn test_build_claude_cmd_includes_model_flag() {
        let cmd = build_claude_cmd(
            "my-session",
            "uuid-123",
            false,
            "claude",
            Some("claude-sonnet-4-6"),
            None,
            None,
        );
        assert!(cmd.contains("--model claude-sonnet-4-6"), "cmd: {cmd}");
    }

    #[test]
    fn test_build_claude_cmd_no_model_flag_when_none() {
        let cmd = build_claude_cmd("my-session", "uuid-123", false, "claude", None, None, None);
        assert!(!cmd.contains("--model"), "cmd: {cmd}");
    }

    #[test]
    fn test_build_claude_cmd_model_before_claude_args() {
        let cmd = build_claude_cmd(
            "my-session",
            "uuid-123",
            false,
            "claude",
            Some("claude-opus-4-6"),
            Some("--extra-arg"),
            None,
        );
        let model_pos = cmd.find("--model").unwrap();
        let args_pos = cmd.find("--extra-arg").unwrap();
        assert!(
            model_pos < args_pos,
            "--model should appear before --extra-arg"
        );
    }

    #[test]
    fn test_build_claude_cmd_worktree_includes_model() {
        let cmd = build_claude_cmd(
            "my-session",
            "uuid-123",
            true,
            "claude",
            Some("claude-haiku-4-5-20251001"),
            None,
            None,
        );
        assert!(cmd.contains("--worktree my-session"), "cmd: {cmd}");
        assert!(
            cmd.contains("--model claude-haiku-4-5-20251001"),
            "cmd: {cmd}"
        );
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

        let result = create_session(name, dir, None, None, "test-uuid-123", true, "claude", None);
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
