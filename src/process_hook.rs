use crate::session::{self, SessionState};
use crate::tmux;
use color_eyre::Result;
use serde::Deserialize;
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};

#[derive(Deserialize)]
struct HookEvent {
    session_id: String,
    hook_event_name: String,
}

fn state_for_event(event_name: &str) -> Option<SessionState> {
    match event_name {
        "Stop" => Some(SessionState::Idle),
        "UserPromptSubmit" => Some(SessionState::Working),
        "PermissionRequest" => Some(SessionState::WaitingUser),
        _ => None,
    }
}

pub fn run() -> Result<()> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;

    // Log the raw input for debugging
    log_input(&input)?;

    let event: HookEvent = serde_json::from_str(&input)?;

    if let Some(state) = state_for_event(&event.hook_event_name) {
        // Silently ignore if the session isn't tracked by us
        let _ = session::update_state_by_claude_session(&event.session_id, state);
    }

    if event.hook_event_name == "SessionStart"
        && let Ok(Some(record)) = session::find_by_claude_session(&event.session_id)
    {
        let _ = tmux::setup_editor_window(&record.tmux_session_name, &record.directory);
    }

    Ok(())
}

fn log_input(input: &str) -> Result<()> {
    let parsed: serde_json::Value = serde_json::from_str(input)?;
    let pretty = serde_json::to_string_pretty(&parsed)?;

    let log_path = dirs::home_dir()
        .ok_or_else(|| color_eyre::eyre::eyre!("Could not determine home directory"))?
        .join(".van-damme")
        .join("debug.log");

    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;

    writeln!(file, "{pretty}")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_state_for_event_stop() {
        assert_eq!(state_for_event("Stop"), Some(SessionState::Idle));
    }

    #[test]
    fn test_state_for_event_user_prompt_submit() {
        assert_eq!(
            state_for_event("UserPromptSubmit"),
            Some(SessionState::Working)
        );
    }

    #[test]
    fn test_state_for_event_permission_request() {
        assert_eq!(
            state_for_event("PermissionRequest"),
            Some(SessionState::WaitingUser)
        );
    }

    #[test]
    fn test_state_for_event_unknown() {
        assert_eq!(state_for_event("SomethingElse"), None);
    }

    #[test]
    fn test_hook_event_deserialization() {
        let json = r#"{"session_id":"abc-123","hook_event_name":"Stop"}"#;
        let event: HookEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.session_id, "abc-123");
        assert_eq!(event.hook_event_name, "Stop");
    }

    #[test]
    fn test_hook_event_deserialization_ignores_extra_fields() {
        let json = r#"{"session_id":"abc-123","hook_event_name":"Stop","extra":"ignored"}"#;
        let event: HookEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.session_id, "abc-123");
    }

    #[test]
    fn test_log_input_writes_to_file() {
        let tmp = tempfile::tempdir().unwrap();
        let log_path = tmp.path().join("debug.log");

        let input = r#"{"session_id":"abc-123","hook_event_name":"Stop"}"#;
        let parsed: serde_json::Value = serde_json::from_str(input).unwrap();
        let pretty = serde_json::to_string_pretty(&parsed).unwrap();

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .unwrap();
        writeln!(file, "{pretty}").unwrap();

        let contents = fs::read_to_string(&log_path).unwrap();
        assert!(contents.contains("abc-123"));
        assert!(contents.contains("Stop"));
    }
}
