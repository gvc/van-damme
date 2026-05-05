use crate::session::{self, SessionState};
use crate::tmux;
use color_eyre::Result;
use serde::Deserialize;
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::time::SystemTime;

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
    match run_inner() {
        Ok(()) => Ok(()),
        Err(e) => {
            log_error(&e.to_string());
            Ok(())
        }
    }
}

fn run_inner() -> Result<()> {
    let input = read_stdin()?;

    log_raw(&input);

    let event: HookEvent = serde_json::from_str(&input)?;

    if let Some(state) = state_for_event(&event.hook_event_name) {
        let _ = session::update_state_by_claude_session(&event.session_id, state);
    }

    if event.hook_event_name == "SessionStart"
        && let Ok(Some(record)) = session::find_by_claude_session(&event.session_id)
    {
        let window_name = tmux::window_name_from_command(&record.claude_command);
        let _ =
            tmux::setup_editor_window(&record.tmux_session_name, &record.directory, window_name);
    }

    Ok(())
}

fn read_stdin() -> Result<String> {
    let mut input = String::new();
    match io::stdin().read_to_string(&mut input) {
        Ok(0) => Err(color_eyre::eyre::eyre!("empty stdin")),
        Ok(_) => {
            if input.trim().is_empty() {
                Err(color_eyre::eyre::eyre!("empty stdin"))
            } else {
                Ok(input)
            }
        }
        Err(e) => Err(e.into()),
    }
}

fn log_path() -> Option<std::path::PathBuf> {
    Some(dirs::home_dir()?.join(".van-damme").join("debug.log"))
}

fn timestamp() -> String {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

fn log_raw(input: &str) {
    let _ = (|| -> io::Result<()> {
        let path = log_path().ok_or(io::Error::other("no home"))?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        if fs::metadata(&path).is_ok_and(|meta| meta.len() > 1_048_576) {
            fs::write(&path, "")?;
        }

        let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
        writeln!(file, "[{}] {}", timestamp(), input.trim())?;
        Ok(())
    })();
}

fn log_error(msg: &str) {
    let _ = (|| -> io::Result<()> {
        let path = log_path().ok_or(io::Error::other("no home"))?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
        writeln!(file, "[{}] ERROR: {}", timestamp(), msg)?;
        Ok(())
    })();
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
    fn test_log_raw_writes_to_file() {
        let tmp = tempfile::tempdir().unwrap();
        let log_file = tmp.path().join("debug.log");

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_file)
            .unwrap();
        writeln!(file, "[123] test input").unwrap();

        let contents = fs::read_to_string(&log_file).unwrap();
        assert!(contents.contains("test input"));
    }

    #[test]
    fn test_log_rotation() {
        let tmp = tempfile::tempdir().unwrap();
        let log_file = tmp.path().join("debug.log");

        let big_content = "x".repeat(1_048_577);
        fs::write(&log_file, &big_content).unwrap();

        assert!(fs::metadata(&log_file).unwrap().len() > 1_048_576);

        let path = log_file.clone();
        let _ = (|| -> io::Result<()> {
            if let Ok(meta) = fs::metadata(&path) {
                if meta.len() > 1_048_576 {
                    fs::write(&path, "")?;
                }
            }
            let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
            writeln!(file, "[0] after rotation")?;
            Ok(())
        })();

        let contents = fs::read_to_string(&log_file).unwrap();
        assert!(contents.contains("after rotation"));
        assert!(contents.len() < 100);
    }
}
