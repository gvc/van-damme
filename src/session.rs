use color_eyre::{Result, eyre::eyre};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum SessionState {
    Working,
    WaitingUser,
    Idle,
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
    #[serde(default = "default_claude_command")]
    pub claude_command: String,
    #[serde(default)]
    pub model_id: Option<String>,
}

fn default_state() -> SessionState {
    SessionState::Idle
}

fn default_claude_command() -> String {
    "claude".to_string()
}

pub struct SessionDb {
    pub sessions: Vec<SessionRecord>,
    file: File,
}

impl SessionDb {
    /// Open the session database at `path`, creating it if missing.
    /// Acquires an exclusive flock for the lifetime of this SessionDb.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;

        let fd = file.as_raw_fd();
        let ret = unsafe { libc::flock(fd, libc::LOCK_EX) };
        if ret != 0 {
            return Err(eyre!(
                "flock failed: {}",
                std::io::Error::last_os_error()
            ));
        }

        let contents = fs::read_to_string(path)?;
        let mut sessions: Vec<SessionRecord> = if contents.trim().is_empty() {
            Vec::new()
        } else {
            let db: SessionDbJson = serde_json::from_str(&contents)?;
            db.sessions
        };

        for s in &mut sessions {
            let t = s.directory.trim_end_matches('/');
            s.directory = if t.is_empty() {
                "/".to_string()
            } else {
                t.to_string()
            };
        }

        Ok(SessionDb { sessions, file })
    }

    /// Write current sessions to disk. Lock remains held.
    pub fn save(&mut self) -> Result<()> {
        let json = serde_json::to_string_pretty(&SessionDbJson {
            sessions: self.sessions.clone(),
        })?;
        self.file.set_len(0)?;
        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(json.as_bytes())?;
        self.file.flush()?;
        Ok(())
    }
}

impl Drop for SessionDb {
    fn drop(&mut self) {
        let fd = self.file.as_raw_fd();
        unsafe { libc::flock(fd, libc::LOCK_UN) };
    }
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct SessionDbJson {
    sessions: Vec<SessionRecord>,
}

/// Returns the default path to ~/.van-damme/sessions.json
pub fn default_db_path() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| eyre!("Could not determine home directory"))?;
    Ok(home.join(".van-damme").join("sessions.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_db_path() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("sessions.json");
        (tmp, path)
    }

    fn make_record(name: &str, claude_id: Option<&str>) -> SessionRecord {
        SessionRecord {
            tmux_session_id: "$1".to_string(),
            tmux_session_name: name.to_string(),
            claude_session_id: claude_id.map(str::to_string),
            directory: "/tmp".to_string(),
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            state: SessionState::Idle,
            claude_command: "claude".to_string(),
            model_id: None,
        }
    }

    #[test]
    fn test_open_creates_file_when_missing() {
        let (_tmp, path) = temp_db_path();
        assert!(!path.exists());
        let db = SessionDb::open(&path).unwrap();
        assert!(db.sessions.is_empty());
        assert!(path.exists());
    }

    #[test]
    fn test_save_and_reload() {
        let (_tmp, path) = temp_db_path();
        {
            let mut db = SessionDb::open(&path).unwrap();
            db.sessions.push(make_record("test", Some("uuid-1")));
            db.save().unwrap();
        }
        let db = SessionDb::open(&path).unwrap();
        assert_eq!(db.sessions.len(), 1);
        assert_eq!(db.sessions[0].tmux_session_name, "test");
    }

    #[test]
    fn test_add_session() {
        let (_tmp, path) = temp_db_path();
        {
            let mut db = SessionDb::open(&path).unwrap();
            db.sessions.push(make_record("first", Some("uuid-first")));
            db.sessions.push(make_record("second", Some("uuid-second")));
            db.save().unwrap();
        }
        let db = SessionDb::open(&path).unwrap();
        assert_eq!(db.sessions.len(), 2);
    }

    #[test]
    fn test_remove_session() {
        let (_tmp, path) = temp_db_path();
        {
            let mut db = SessionDb::open(&path).unwrap();
            db.sessions.push(make_record("first", Some("uuid-first")));
            db.sessions.push(make_record("second", Some("uuid-second")));
            db.save().unwrap();
        }
        {
            let mut db = SessionDb::open(&path).unwrap();
            db.sessions.retain(|s| s.tmux_session_name != "first");
            db.save().unwrap();
        }
        let db = SessionDb::open(&path).unwrap();
        assert_eq!(db.sessions.len(), 1);
        assert_eq!(db.sessions[0].tmux_session_name, "second");
    }

    #[test]
    fn test_update_session_state() {
        let (_tmp, path) = temp_db_path();
        {
            let mut db = SessionDb::open(&path).unwrap();
            db.sessions.push(make_record("my-session", Some("uuid-1")));
            db.save().unwrap();
        }
        {
            let mut db = SessionDb::open(&path).unwrap();
            let s = db
                .sessions
                .iter_mut()
                .find(|s| s.claude_session_id.as_deref() == Some("uuid-1"))
                .unwrap();
            s.state = SessionState::Working;
            db.save().unwrap();
        }
        let db = SessionDb::open(&path).unwrap();
        assert_eq!(db.sessions[0].state, SessionState::Working);
    }

    #[test]
    fn test_update_tmux_session_id() {
        let (_tmp, path) = temp_db_path();
        {
            let mut db = SessionDb::open(&path).unwrap();
            let mut r = make_record("my-session", Some("uuid-1"));
            r.tmux_session_id = String::new();
            db.sessions.push(r);
            db.save().unwrap();
        }
        {
            let mut db = SessionDb::open(&path).unwrap();
            let s = db
                .sessions
                .iter_mut()
                .find(|s| s.tmux_session_name == "my-session")
                .unwrap();
            s.tmux_session_id = "$42".to_string();
            db.save().unwrap();
        }
        let db = SessionDb::open(&path).unwrap();
        assert_eq!(db.sessions[0].tmux_session_id, "$42");
    }

    #[test]
    fn test_open_normalizes_trailing_slash() {
        let (_tmp, path) = temp_db_path();
        let json = r#"{"sessions":[{
            "tmux_session_id": "$1",
            "tmux_session_name": "nous",
            "claude_session_id": null,
            "directory": "/home/user/code/nous/",
            "created_at": 100,
            "state": "Idle",
            "claude_command": "claude"
        }]}"#;
        std::fs::write(&path, json).unwrap();
        let db = SessionDb::open(&path).unwrap();
        assert_eq!(db.sessions[0].directory, "/home/user/code/nous");
    }

    #[test]
    fn test_open_preserves_root_slash() {
        let (_tmp, path) = temp_db_path();
        let json = r#"{"sessions":[{
            "tmux_session_id": "$1",
            "tmux_session_name": "root",
            "claude_session_id": null,
            "directory": "/",
            "created_at": 100,
            "state": "Idle",
            "claude_command": "claude"
        }]}"#;
        std::fs::write(&path, json).unwrap();
        let db = SessionDb::open(&path).unwrap();
        assert_eq!(db.sessions[0].directory, "/");
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
            claude_command: "claude".to_string(),
            model_id: None,
        };
        let json = serde_json::to_string(&record).unwrap();
        let deserialized: SessionRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(record, deserialized);
    }

    #[test]
    fn test_state_defaults_to_idle_on_deserialize() {
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

    #[test]
    fn test_claude_command_defaults_on_deserialize() {
        let json = r#"{
            "tmux_session_id": "$1",
            "tmux_session_name": "legacy",
            "claude_session_id": null,
            "directory": "/tmp",
            "created_at": 100
        }"#;
        let record: SessionRecord = serde_json::from_str(json).unwrap();
        assert_eq!(record.claude_command, "claude");
    }

    #[test]
    fn test_model_id_defaults_to_none_on_deserialize() {
        let json = r#"{
            "tmux_session_id": "$1",
            "tmux_session_name": "legacy",
            "claude_session_id": null,
            "directory": "/tmp",
            "created_at": 100
        }"#;
        let record: SessionRecord = serde_json::from_str(json).unwrap();
        assert_eq!(record.model_id, None);
    }

    #[test]
    fn test_session_state_display() {
        assert_eq!(SessionState::Working.to_string(), "Working");
        assert_eq!(SessionState::WaitingUser.to_string(), "Waiting User");
        assert_eq!(SessionState::Idle.to_string(), "Idle");
    }

    #[test]
    fn test_model_id_roundtrip() {
        let (_tmp, path) = temp_db_path();
        {
            let mut db = SessionDb::open(&path).unwrap();
            let mut r = make_record("model-session", Some("uuid-123"));
            r.model_id = Some("claude-sonnet-4-6".to_string());
            db.sessions.push(r);
            db.save().unwrap();
        }
        let db = SessionDb::open(&path).unwrap();
        assert_eq!(
            db.sessions[0].model_id,
            Some("claude-sonnet-4-6".to_string())
        );
    }
}
