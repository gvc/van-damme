use color_eyre::{Result, eyre::eyre};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SessionRecord {
    pub tmux_session_id: String,
    pub tmux_session_name: String,
    pub claude_session_id: Option<String>,
    pub directory: String,
    pub created_at: u64,
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

/// Add a new session record and persist to disk.
pub fn add_session(
    tmux_session_id: String,
    tmux_session_name: String,
    directory: String,
) -> Result<SessionRecord> {
    let path = default_db_path()?;
    add_session_to(&path, tmux_session_id, tmux_session_name, directory)
}

fn add_session_to(
    path: &Path,
    tmux_session_id: String,
    tmux_session_name: String,
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
        claude_session_id: None,
        directory,
        created_at,
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
            "/tmp".to_string(),
        )
        .unwrap();

        assert_eq!(record.tmux_session_name, "test-session");
        assert!(record.created_at > 0);
        assert!(record.claude_session_id.is_none());

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
            "/tmp".to_string(),
        )
        .unwrap();
        add_session_to(
            &path,
            "$2".to_string(),
            "second".to_string(),
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
            "/tmp".to_string(),
        )
        .unwrap();
        add_session_to(
            &path,
            "$2".to_string(),
            "second".to_string(),
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
            "/tmp".to_string(),
        )
        .unwrap();
        add_session_to(
            &path,
            "$2".to_string(),
            "second".to_string(),
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
            }],
        };
        save_db_to(&path, &db).unwrap();
        let loaded = load_db_from(&path).unwrap();
        assert_eq!(loaded.sessions.len(), 1);
        assert_eq!(loaded.sessions[0], db.sessions[0]);
    }

}
