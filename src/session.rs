use color_eyre::{Result, eyre::eyre};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum SessionState {
    Working,
    WaitingUser,
    Idle,
}

impl SessionState {
    pub fn icon(&self) -> &'static str {
        match self {
            SessionState::Working => "⚙",
            SessionState::WaitingUser => "⏳",
            SessionState::Idle => "●",
        }
    }
}

impl std::fmt::Display for SessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionState::Working => write!(f, "Working"),
            SessionState::WaitingUser => write!(f, "Waiting User"),
            SessionState::Idle => write!(f, "Idle"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SessionRecord {
    pub tmux_session_id: String,
    pub tmux_session_name: String,
    pub claude_session_id: Option<String>,
    pub directory: String,
    pub created_at: u64,
    #[serde(default = "default_state")]
    pub state: SessionState,
}

fn default_state() -> SessionState {
    SessionState::Idle
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct SessionDb {
    pub sessions: Vec<SessionRecord>,
}

/// Returns the default path to ~/.van-damme/sessions.json
fn default_db_path() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| eyre!("Could not determine home directory"))?;
    Ok(home.join(".van-damme").join("sessions.json"))
}

/// Load the session database from the given path. Returns empty DB if file doesn't exist.
fn load_db_from(path: &Path) -> Result<SessionDb> {
    if !path.exists() {
        return Ok(SessionDb::default());
    }
    let contents = fs::read_to_string(path)?;
    let db: SessionDb = serde_json::from_str(&contents)?;
    Ok(db)
}

/// Save the session database to the given path, creating parent dirs as needed.
fn save_db_to(path: &Path, db: &SessionDb) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(db)?;
    fs::write(path, json)?;
    Ok(())
}

/// Load all session records from disk.
pub fn list_sessions() -> Result<Vec<SessionRecord>> {
    let path = default_db_path()?;
    list_sessions_from(&path)
}

/// Find a session record by its claude session id.
pub fn find_by_claude_session(claude_session_id: &str) -> Result<Option<SessionRecord>> {
    let path = default_db_path()?;
    let db = load_db_from(&path)?;
    Ok(db
        .sessions
        .into_iter()
        .find(|s| s.claude_session_id.as_deref() == Some(claude_session_id)))
}

fn list_sessions_from(path: &Path) -> Result<Vec<SessionRecord>> {
    let db = load_db_from(path)?;
    Ok(db.sessions)
}

/// Remove a session record by tmux session name and persist to disk.
pub fn remove_session(tmux_session_name: &str) -> Result<()> {
    let path = default_db_path()?;
    remove_session_from(&path, tmux_session_name)
}

fn remove_session_from(path: &Path, tmux_session_name: &str) -> Result<()> {
    let mut db = load_db_from(path)?;
    db.sessions
        .retain(|s| s.tmux_session_name != tmux_session_name);
    save_db_to(path, &db)?;
    Ok(())
}

/// Update the state of a session by claude session id.
pub fn update_state_by_claude_session(claude_session_id: &str, state: SessionState) -> Result<()> {
    let path = default_db_path()?;
    update_state_by_claude_session_at(&path, claude_session_id, state)
}

fn update_state_by_claude_session_at(
    path: &Path,
    claude_session_id: &str,
    state: SessionState,
) -> Result<()> {
    let mut db = load_db_from(path)?;
    let session = db
        .sessions
        .iter_mut()
        .find(|s| s.claude_session_id.as_deref() == Some(claude_session_id))
        .ok_or_else(|| eyre!("No session with claude_session_id '{}'", claude_session_id))?;
    session.state = state;
    save_db_to(path, &db)?;
    Ok(())
}

/// Update the tmux session ID for a session looked up by tmux session name.
pub fn update_tmux_session_id(tmux_session_name: &str, tmux_session_id: &str) -> Result<()> {
    let path = default_db_path()?;
    update_tmux_session_id_at(&path, tmux_session_name, tmux_session_id)
}

fn update_tmux_session_id_at(
    path: &Path,
    tmux_session_name: &str,
    tmux_session_id: &str,
) -> Result<()> {
    let mut db = load_db_from(path)?;
    let session = db
        .sessions
        .iter_mut()
        .find(|s| s.tmux_session_name == tmux_session_name)
        .ok_or_else(|| eyre!("No session with tmux_session_name '{}'", tmux_session_name))?;
    session.tmux_session_id = tmux_session_id.to_string();
    save_db_to(path, &db)?;
    Ok(())
}

/// Add a new session record and persist to disk.
pub fn add_session(
    tmux_session_id: String,
    tmux_session_name: String,
    claude_session_id: String,
    directory: String,
) -> Result<SessionRecord> {
    let path = default_db_path()?;
    add_session_to(
        &path,
        tmux_session_id,
        tmux_session_name,
        claude_session_id,
        directory,
    )
}

fn add_session_to(
    path: &Path,
    tmux_session_id: String,
    tmux_session_name: String,
    claude_session_id: String,
    directory: String,
) -> Result<SessionRecord> {
    let mut db = load_db_from(path)?;
    let created_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let record = SessionRecord {
        tmux_session_id,
        tmux_session_name,
        claude_session_id: Some(claude_session_id),
        directory,
        created_at,
        state: SessionState::Idle,
    };

    db.sessions.push(record.clone());
    save_db_to(path, &db)?;
    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db_path() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("sessions.json");
        (tmp, path)
    }

    #[test]
    fn test_session_record_serialization_roundtrip() {
        let record = SessionRecord {
            tmux_session_id: "$1".to_string(),
            tmux_session_name: "my-task".to_string(),
            claude_session_id: None,
            directory: "/tmp".to_string(),
            created_at: 1700000000,
            state: SessionState::Idle,
        };
        let json = serde_json::to_string(&record).unwrap();
        let deserialized: SessionRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(record, deserialized);
    }

    #[test]
    fn test_session_db_roundtrip() {
        let db = SessionDb {
            sessions: vec![SessionRecord {
                tmux_session_id: "$1".to_string(),
                tmux_session_name: "test".to_string(),
                claude_session_id: Some("sess-123".to_string()),
                directory: "/home/user".to_string(),
                created_at: 1700000000,
                state: SessionState::Idle,
            }],
        };
        let json = serde_json::to_string_pretty(&db).unwrap();
        let deserialized: SessionDb = serde_json::from_str(&json).unwrap();
        assert_eq!(db.sessions.len(), deserialized.sessions.len());
        assert_eq!(db.sessions[0], deserialized.sessions[0]);
    }

    #[test]
    fn test_load_db_empty_when_no_file() {
        let (_tmp, path) = temp_db_path();
        let db = load_db_from(&path).unwrap();
        assert!(db.sessions.is_empty());
    }

    #[test]
    fn test_add_session_creates_file() {
        let (_tmp, path) = temp_db_path();
        let record = add_session_to(
            &path,
            "$1".to_string(),
            "test-session".to_string(),
            "test-uuid-123".to_string(),
            "/tmp".to_string(),
        )
        .unwrap();

        assert_eq!(record.tmux_session_name, "test-session");
        assert!(record.created_at > 0);
        assert_eq!(record.claude_session_id, Some("test-uuid-123".to_string()));

        // Verify persistence
        let db = load_db_from(&path).unwrap();
        assert_eq!(db.sessions.len(), 1);
        assert_eq!(db.sessions[0].tmux_session_name, "test-session");
    }

    #[test]
    fn test_add_multiple_sessions() {
        let (_tmp, path) = temp_db_path();
        add_session_to(
            &path,
            "$1".to_string(),
            "first".to_string(),
            "uuid-first".to_string(),
            "/tmp".to_string(),
        )
        .unwrap();
        add_session_to(
            &path,
            "$2".to_string(),
            "second".to_string(),
            "uuid-second".to_string(),
            "/tmp".to_string(),
        )
        .unwrap();

        let db = load_db_from(&path).unwrap();
        assert_eq!(db.sessions.len(), 2);
    }

    #[test]
    fn test_list_sessions_from() {
        let (_tmp, path) = temp_db_path();
        add_session_to(
            &path,
            "$1".to_string(),
            "first".to_string(),
            "uuid-first".to_string(),
            "/tmp".to_string(),
        )
        .unwrap();
        add_session_to(
            &path,
            "$2".to_string(),
            "second".to_string(),
            "uuid-second".to_string(),
            "/tmp".to_string(),
        )
        .unwrap();

        let sessions = list_sessions_from(&path).unwrap();
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].tmux_session_name, "first");
        assert_eq!(sessions[1].tmux_session_name, "second");
    }

    #[test]
    fn test_list_sessions_empty() {
        let (_tmp, path) = temp_db_path();
        let sessions = list_sessions_from(&path).unwrap();
        assert!(sessions.is_empty());
    }

    #[test]
    fn test_remove_session() {
        let (_tmp, path) = temp_db_path();
        add_session_to(
            &path,
            "$1".to_string(),
            "first".to_string(),
            "uuid-first".to_string(),
            "/tmp".to_string(),
        )
        .unwrap();
        add_session_to(
            &path,
            "$2".to_string(),
            "second".to_string(),
            "uuid-second".to_string(),
            "/tmp".to_string(),
        )
        .unwrap();

        remove_session_from(&path, "first").unwrap();

        let sessions = list_sessions_from(&path).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].tmux_session_name, "second");
    }

    #[test]
    fn test_remove_nonexistent_session() {
        let (_tmp, path) = temp_db_path();
        add_session_to(
            &path,
            "$1".to_string(),
            "first".to_string(),
            "uuid-first".to_string(),
            "/tmp".to_string(),
        )
        .unwrap();

        remove_session_from(&path, "nonexistent").unwrap();

        let sessions = list_sessions_from(&path).unwrap();
        assert_eq!(sessions.len(), 1);
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let (_tmp, path) = temp_db_path();
        let db = SessionDb {
            sessions: vec![SessionRecord {
                tmux_session_id: "$5".to_string(),
                tmux_session_name: "roundtrip".to_string(),
                claude_session_id: None,
                directory: "/home".to_string(),
                created_at: 999,
                state: SessionState::Working,
            }],
        };
        save_db_to(&path, &db).unwrap();
        let loaded = load_db_from(&path).unwrap();
        assert_eq!(loaded.sessions.len(), 1);
        assert_eq!(loaded.sessions[0], db.sessions[0]);
    }

    #[test]
    fn test_update_session_state() {
        let (_tmp, path) = temp_db_path();
        add_session_to(
            &path,
            "$1".to_string(),
            "my-session".to_string(),
            "uuid-1".to_string(),
            "/tmp".to_string(),
        )
        .unwrap();

        // Default state is Idle
        let sessions = list_sessions_from(&path).unwrap();
        assert_eq!(sessions[0].state, SessionState::Idle);

        // Update to Working (lookup by claude_session_id)
        update_state_by_claude_session_at(&path, "uuid-1", SessionState::Working).unwrap();
        let sessions = list_sessions_from(&path).unwrap();
        assert_eq!(sessions[0].state, SessionState::Working);

        // Update to WaitingUser
        update_state_by_claude_session_at(&path, "uuid-1", SessionState::WaitingUser).unwrap();
        let sessions = list_sessions_from(&path).unwrap();
        assert_eq!(sessions[0].state, SessionState::WaitingUser);
    }

    #[test]
    fn test_update_session_state_not_found() {
        let (_tmp, path) = temp_db_path();
        let result = update_state_by_claude_session_at(&path, "nonexistent", SessionState::Working);
        assert!(result.is_err());
    }

    #[test]
    fn test_session_state_display() {
        assert_eq!(SessionState::Working.to_string(), "Working");
        assert_eq!(SessionState::WaitingUser.to_string(), "Waiting User");
        assert_eq!(SessionState::Idle.to_string(), "Idle");
    }

    #[test]
    fn test_update_tmux_session_id() {
        let (_tmp, path) = temp_db_path();
        add_session_to(
            &path,
            "".to_string(),
            "my-session".to_string(),
            "uuid-1".to_string(),
            "/tmp".to_string(),
        )
        .unwrap();

        // Initially empty
        let sessions = list_sessions_from(&path).unwrap();
        assert_eq!(sessions[0].tmux_session_id, "");

        // Update to real tmux session ID
        update_tmux_session_id_at(&path, "my-session", "$42").unwrap();
        let sessions = list_sessions_from(&path).unwrap();
        assert_eq!(sessions[0].tmux_session_id, "$42");
    }

    #[test]
    fn test_update_tmux_session_id_not_found() {
        let (_tmp, path) = temp_db_path();
        let result = update_tmux_session_id_at(&path, "nonexistent", "$1");
        assert!(result.is_err());
    }

    #[test]
    fn test_state_defaults_to_idle_on_deserialize() {
        // Simulate a legacy record without a state field
        let json = r#"{
            "tmux_session_id": "$1",
            "tmux_session_name": "legacy",
            "claude_session_id": null,
            "directory": "/tmp",
            "created_at": 100
        }"#;
        let record: SessionRecord = serde_json::from_str(json).unwrap();
        assert_eq!(record.state, SessionState::Idle);
    }
}
