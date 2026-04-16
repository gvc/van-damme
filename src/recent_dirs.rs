use color_eyre::{Result, eyre::eyre};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
struct DirEntry {
    path: String,
    last_used: u64,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct RecentDirsDb {
    directories: Vec<DirEntry>,
}

fn default_db_path() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| eyre!("Could not determine home directory"))?;
    Ok(home.join(".van-damme").join("recent_dirs.json"))
}

fn load_db_from(path: &Path) -> Result<RecentDirsDb> {
    if !path.exists() {
        return Ok(RecentDirsDb::default());
    }
    let contents = fs::read_to_string(path)?;
    let db: RecentDirsDb = serde_json::from_str(&contents)?;
    Ok(db)
}

fn save_db_to(path: &Path, db: &RecentDirsDb) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(db)?;
    fs::write(path, json)?;
    Ok(())
}

/// Record a directory as recently used. Updates the timestamp if it already exists.
pub fn record_directory(directory: &str) -> Result<()> {
    let path = default_db_path()?;
    record_directory_to(&path, directory)
}

/// Normalize a directory path by stripping all trailing slashes (unless root "/").
fn normalize_dir(directory: &str) -> String {
    let trimmed = directory.trim_end_matches('/');
    if trimmed.is_empty() {
        "/".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Returns true if a directory should be excluded from the listing.
fn is_excluded_dir(directory: &str) -> bool {
    directory.starts_with("/private") || directory.contains("/.claude/")
}

fn record_directory_to(path: &Path, directory: &str) -> Result<()> {
    let normalized = normalize_dir(directory);

    if is_excluded_dir(&normalized) {
        return Ok(());
    }

    let mut db = load_db_from(path)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Update timestamp if exists, otherwise add new entry
    if let Some(entry) = db.directories.iter_mut().find(|e| e.path == normalized) {
        entry.last_used = now;
    } else {
        db.directories.push(DirEntry {
            path: normalized,
            last_used: now,
        });
    }

    save_db_to(path, &db)?;
    Ok(())
}

/// Return up to `limit` most recently used directories, ordered by most recent first.
pub fn recent_directories(limit: usize) -> Result<Vec<String>> {
    let path = default_db_path()?;
    recent_directories_from(&path, limit)
}

fn recent_directories_from(path: &Path, limit: usize) -> Result<Vec<String>> {
    let db = load_db_from(path)?;
    let mut entries = db.directories;
    entries.sort_by(|a, b| b.last_used.cmp(&a.last_used));

    let mut seen = HashSet::new();
    let mut dirs = Vec::new();
    for e in entries {
        let normalized = normalize_dir(&e.path);
        if is_excluded_dir(&normalized) {
            continue;
        }
        if seen.insert(normalized.clone()) {
            dirs.push(normalized);
            if dirs.len() >= limit {
                break;
            }
        }
    }
    Ok(dirs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db_path() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("recent_dirs.json");
        (tmp, path)
    }

    #[test]
    fn test_record_and_retrieve() {
        let (_tmp, path) = temp_db_path();
        let db = RecentDirsDb {
            directories: vec![
                DirEntry {
                    path: "/home/user/project".to_string(),
                    last_used: 100,
                },
                DirEntry {
                    path: "/tmp".to_string(),
                    last_used: 200,
                },
            ],
        };
        save_db_to(&path, &db).unwrap();

        let dirs = recent_directories_from(&path, 5).unwrap();
        assert_eq!(dirs.len(), 2);
        // Most recent first
        assert_eq!(dirs[0], "/tmp");
        assert_eq!(dirs[1], "/home/user/project");
    }

    #[test]
    fn test_record_updates_timestamp() {
        let (_tmp, path) = temp_db_path();

        // Manually create entries with known timestamps
        let db = RecentDirsDb {
            directories: vec![
                DirEntry {
                    path: "/old".to_string(),
                    last_used: 100,
                },
                DirEntry {
                    path: "/newer".to_string(),
                    last_used: 200,
                },
            ],
        };
        save_db_to(&path, &db).unwrap();

        // Re-record /old — should bump its timestamp to now
        record_directory_to(&path, "/old").unwrap();

        let dirs = recent_directories_from(&path, 5).unwrap();
        // /old should now be first since its timestamp was updated
        assert_eq!(dirs[0], "/old");
        assert_eq!(dirs[1], "/newer");
    }

    #[test]
    fn test_deduplicates() {
        let (_tmp, path) = temp_db_path();
        record_directory_to(&path, "/tmp").unwrap();
        record_directory_to(&path, "/tmp").unwrap();

        let db = load_db_from(&path).unwrap();
        assert_eq!(db.directories.len(), 1);
    }

    #[test]
    fn test_respects_limit() {
        let (_tmp, path) = temp_db_path();
        let db = RecentDirsDb {
            directories: vec![
                DirEntry {
                    path: "/a".to_string(),
                    last_used: 100,
                },
                DirEntry {
                    path: "/b".to_string(),
                    last_used: 200,
                },
                DirEntry {
                    path: "/c".to_string(),
                    last_used: 300,
                },
            ],
        };
        save_db_to(&path, &db).unwrap();

        let dirs = recent_directories_from(&path, 2).unwrap();
        assert_eq!(dirs.len(), 2);
        assert_eq!(dirs, vec!["/c", "/b"]);
    }

    #[test]
    fn test_empty_db() {
        let (_tmp, path) = temp_db_path();
        let dirs = recent_directories_from(&path, 5).unwrap();
        assert!(dirs.is_empty());
    }

    #[test]
    fn test_ordered_by_most_recent() {
        let (_tmp, path) = temp_db_path();
        let db = RecentDirsDb {
            directories: vec![
                DirEntry {
                    path: "/old".to_string(),
                    last_used: 100,
                },
                DirEntry {
                    path: "/new".to_string(),
                    last_used: 200,
                },
            ],
        };
        save_db_to(&path, &db).unwrap();

        let dirs = recent_directories_from(&path, 5).unwrap();
        assert_eq!(dirs, vec!["/new", "/old"]);
    }

    #[test]
    fn test_serialization_roundtrip() {
        let db = RecentDirsDb {
            directories: vec![DirEntry {
                path: "/home".to_string(),
                last_used: 1700000000,
            }],
        };
        let json = serde_json::to_string_pretty(&db).unwrap();
        let deserialized: RecentDirsDb = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.directories.len(), 1);
        assert_eq!(deserialized.directories[0], db.directories[0]);
    }

    #[test]
    fn test_normalize_trailing_slashes() {
        assert_eq!(normalize_dir("/home/user/project///"), "/home/user/project");
        assert_eq!(normalize_dir("/home/user/project/"), "/home/user/project");
        assert_eq!(normalize_dir("/home/user/project"), "/home/user/project");
        assert_eq!(normalize_dir("/"), "/");
        assert_eq!(normalize_dir("///"), "/");
    }

    #[test]
    fn test_record_normalizes_trailing_slashes() {
        let (_tmp, path) = temp_db_path();
        record_directory_to(&path, "/home/user/project///").unwrap();

        let dirs = recent_directories_from(&path, 5).unwrap();
        assert_eq!(dirs, vec!["/home/user/project"]);
    }

    #[test]
    fn test_record_merges_slash_variants() {
        let (_tmp, path) = temp_db_path();
        record_directory_to(&path, "/home/user/project").unwrap();
        record_directory_to(&path, "/home/user/project/").unwrap();
        record_directory_to(&path, "/home/user/project///").unwrap();

        let db = load_db_from(&path).unwrap();
        assert_eq!(db.directories.len(), 1);
        assert_eq!(db.directories[0].path, "/home/user/project");
    }

    #[test]
    fn test_excludes_claude_directories() {
        let (_tmp, path) = temp_db_path();
        record_directory_to(&path, "/home/user/project/.claude/worktrees/my-branch").unwrap();
        record_directory_to(&path, "/home/user/project").unwrap();

        let dirs = recent_directories_from(&path, 5).unwrap();
        assert_eq!(dirs, vec!["/home/user/project"]);
    }

    #[test]
    fn test_excludes_private_directories() {
        let (_tmp, path) = temp_db_path();
        record_directory_to(&path, "/private/var/folders/xd/something").unwrap();
        record_directory_to(&path, "/home/user/project").unwrap();

        let dirs = recent_directories_from(&path, 5).unwrap();
        assert_eq!(dirs, vec!["/home/user/project"]);
    }

    #[test]
    fn test_retrieval_filters_existing_excluded_entries() {
        let (_tmp, path) = temp_db_path();
        // Simulate pre-existing bad entries in DB
        let db = RecentDirsDb {
            directories: vec![
                DirEntry {
                    path: "/private/var/tmp/something".to_string(),
                    last_used: 300,
                },
                DirEntry {
                    path: "/home/user/.claude/worktrees/branch".to_string(),
                    last_used: 200,
                },
                DirEntry {
                    path: "/home/user/project////////".to_string(),
                    last_used: 100,
                },
                DirEntry {
                    path: "/home/user/project".to_string(),
                    last_used: 50,
                },
            ],
        };
        save_db_to(&path, &db).unwrap();

        let dirs = recent_directories_from(&path, 10).unwrap();
        // Should only have one entry: normalized, deduped, no excluded dirs
        assert_eq!(dirs, vec!["/home/user/project"]);
    }
}
