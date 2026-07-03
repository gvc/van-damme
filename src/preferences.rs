use color_eyre::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::theme::{self, Theme};

#[derive(Debug, Serialize, Deserialize, Default)]
struct Preferences {
    #[serde(default)]
    pub last_model: Option<String>,
    #[serde(default)]
    pub theme: Option<String>,
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

/// Return the mtime of preferences.json, or None if missing/unreadable.
pub fn prefs_mtime() -> Option<std::time::SystemTime> {
    let path = default_prefs_path().ok()?;
    std::fs::metadata(&path).ok()?.modified().ok()
}

pub fn themes_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".van-damme").join("themes"))
}

/// Load Theme from ~/.van-damme/themes/<name>.toml. Falls back to SYNDICATE.
pub fn load_theme() -> Theme {
    load_theme_inner().unwrap_or_else(|| theme::SYNDICATE.clone())
}

fn load_theme_inner() -> Option<Theme> {
    let path = default_prefs_path().ok()?;
    let prefs = load_prefs_from(&path).ok()?;
    let name = prefs.theme?;
    let theme_path = themes_dir()?.join(format!("{name}.toml"));
    Some(theme::parse_theme_file(&theme_path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_path() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("preferences.json");
        (tmp, path)
    }

    fn load_theme_from(prefs_path: &Path) -> Theme {
        let prefs = load_prefs_from(prefs_path).unwrap_or_default();
        let name = match prefs.theme {
            Some(n) => n,
            None => return theme::SYNDICATE.clone(),
        };
        let theme_path = prefs_path
            .parent()
            .unwrap()
            .join("themes")
            .join(format!("{name}.toml"));
        theme::parse_theme_file(&theme_path)
    }

    #[test]
    fn test_load_theme_no_theme_field_returns_syndicate() {
        let (_tmp, path) = temp_path();
        save_last_model_to(&path, Some("sonnet")).unwrap();
        let t = load_theme_from(&path);
        assert_eq!(t.bg, theme::SYNDICATE.bg);
    }

    #[test]
    fn test_load_theme_missing_file_returns_syndicate() {
        let (_tmp, path) = temp_path();
        let mut prefs = Preferences::default();
        prefs.theme = Some("nonexistent".to_string());
        save_prefs_to(&path, &prefs).unwrap();
        let t = load_theme_from(&path);
        assert_eq!(t.bg, theme::SYNDICATE.bg);
    }

    #[test]
    fn test_load_theme_reads_toml_file() {
        let tmp = tempfile::tempdir().unwrap();
        let prefs_path = tmp.path().join("preferences.json");
        let themes_dir = tmp.path().join("themes");
        std::fs::create_dir_all(&themes_dir).unwrap();
        let theme_path = themes_dir.join("mytest.toml");
        let mut f = std::fs::File::create(&theme_path).unwrap();
        writeln!(f, "bg = \"#ff0000\"").unwrap();

        let mut prefs = Preferences::default();
        prefs.theme = Some("mytest".to_string());
        save_prefs_to(&prefs_path, &prefs).unwrap();

        let t = load_theme_from(&prefs_path);
        assert_eq!(t.bg, ratatui::style::Color::Rgb(0xff, 0x00, 0x00));
        assert_eq!(t.text, theme::SYNDICATE.text);
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
        save_last_model_to(&path, Some("sonnet")).unwrap();
        let prefs = load_prefs_from(&path).unwrap();
        assert_eq!(prefs.last_model, Some("sonnet".to_string()));
    }

    #[test]
    fn test_save_none_clears_model() {
        let (_tmp, path) = temp_path();
        save_last_model_to(&path, Some("opus")).unwrap();
        save_last_model_to(&path, None).unwrap();
        let prefs = load_prefs_from(&path).unwrap();
        assert!(prefs.last_model.is_none());
    }

    #[test]
    fn test_roundtrip_preserves_value() {
        let (_tmp, path) = temp_path();
        save_last_model_to(&path, Some("haiku")).unwrap();
        let prefs = load_prefs_from(&path).unwrap();
        assert_eq!(
            prefs.last_model,
            Some("haiku".to_string())
        );
    }

    #[test]
    fn test_save_overwrites_previous() {
        let (_tmp, path) = temp_path();
        save_last_model_to(&path, Some("opus")).unwrap();
        save_last_model_to(&path, Some("sonnet")).unwrap();
        let prefs = load_prefs_from(&path).unwrap();
        assert_eq!(prefs.last_model, Some("sonnet".to_string()));
    }
}
