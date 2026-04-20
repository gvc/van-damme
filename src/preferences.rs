use color_eyre::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize, Default)]
struct Preferences {
    #[serde(default)]
    pub last_model: Option<String>,
}

fn default_prefs_path() -> Result<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| color_eyre::eyre::eyre!("Could not determine home directory"))?;
    Ok(home.join(".van-damme").join("preferences.json"))
}

fn load_prefs_from(path: &Path) -> Result<Preferences> {
    if !path.exists() {
        return Ok(Preferences::default());
    }
    let contents = fs::read_to_string(path)?;
    let prefs: Preferences = serde_json::from_str(&contents)?;
    Ok(prefs)
}

fn save_prefs_to(path: &Path, prefs: &Preferences) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(prefs)?;
    fs::write(path, json)?;
    Ok(())
}

/// Load the last used model ID from preferences. Returns None if not set or file missing.
pub fn load_last_model() -> Option<String> {
    let path = default_prefs_path().ok()?;
    load_prefs_from(&path).ok()?.last_model
}

/// Persist the last used model ID to preferences. Pass None to clear it.
pub fn save_last_model(model_id: Option<&str>) -> Result<()> {
    let path = default_prefs_path()?;
    save_last_model_to(&path, model_id)
}

fn save_last_model_to(path: &Path, model_id: Option<&str>) -> Result<()> {
    let mut prefs = load_prefs_from(path).unwrap_or_default();
    prefs.last_model = model_id.map(str::to_string);
    save_prefs_to(path, &prefs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("preferences.json");
        (tmp, path)
    }

    #[test]
    fn test_load_missing_file_returns_none() {
        let (_tmp, path) = temp_path();
        let prefs = load_prefs_from(&path).unwrap();
        assert!(prefs.last_model.is_none());
    }

    #[test]
    fn test_save_and_load_model_id() {
        let (_tmp, path) = temp_path();
        save_last_model_to(&path, Some("claude-sonnet-4-6")).unwrap();
        let prefs = load_prefs_from(&path).unwrap();
        assert_eq!(prefs.last_model, Some("claude-sonnet-4-6".to_string()));
    }

    #[test]
    fn test_save_none_clears_model() {
        let (_tmp, path) = temp_path();
        save_last_model_to(&path, Some("claude-opus-4-6")).unwrap();
        save_last_model_to(&path, None).unwrap();
        let prefs = load_prefs_from(&path).unwrap();
        assert!(prefs.last_model.is_none());
    }

    #[test]
    fn test_roundtrip_preserves_value() {
        let (_tmp, path) = temp_path();
        save_last_model_to(&path, Some("claude-haiku-4-5-20251001")).unwrap();
        let prefs = load_prefs_from(&path).unwrap();
        assert_eq!(
            prefs.last_model,
            Some("claude-haiku-4-5-20251001".to_string())
        );
    }

    #[test]
    fn test_save_overwrites_previous() {
        let (_tmp, path) = temp_path();
        save_last_model_to(&path, Some("claude-opus-4-6")).unwrap();
        save_last_model_to(&path, Some("claude-sonnet-4-6")).unwrap();
        let prefs = load_prefs_from(&path).unwrap();
        assert_eq!(prefs.last_model, Some("claude-sonnet-4-6".to_string()));
    }
}
