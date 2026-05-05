use color_eyre::Result;
use serde_json::{Map, Value, json};
use std::fs;
use std::path::{Path, PathBuf};

const VD_COMMAND: &str = "vd process-hook";
const LEGACY_COMMAND: &str = "van-damme process-hook";
const REQUIRED_EVENTS: &[&str] = &[
    "SessionStart",
    "Stop",
    "UserPromptSubmit",
    "PermissionRequest",
];

pub struct InstallReport {
    pub data_dir_created: bool,
    pub settings_created: bool,
    pub hooks_added: Vec<String>,
    pub hooks_already_present: Vec<String>,
    pub hooks_upgraded: Vec<String>,
}

pub struct UninstallReport {
    pub hooks_removed: Vec<String>,
    pub hooks_not_found: Vec<String>,
}

fn claude_settings_path() -> Result<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| color_eyre::eyre::eyre!("Could not determine home directory"))?;
    Ok(home.join(".claude").join("settings.json"))
}

fn vd_data_dir() -> Result<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| color_eyre::eyre::eyre!("Could not determine home directory"))?;
    Ok(home.join(".van-damme"))
}

fn load_settings(path: &Path) -> Result<Map<String, Value>> {
    if !path.exists() {
        return Ok(Map::new());
    }
    let contents = fs::read_to_string(path)?;
    let val: Value = serde_json::from_str(&contents)?;
    match val {
        Value::Object(map) => Ok(map),
        _ => Err(color_eyre::eyre::eyre!(
            "settings.json is not a JSON object"
        )),
    }
}

fn save_settings(path: &Path, settings: &Map<String, Value>) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(&Value::Object(settings.clone()))?;
    fs::write(path, format!("{json}\n"))?;
    Ok(())
}

fn build_vd_matcher_group() -> Value {
    json!({
        "matcher": "",
        "hooks": [{ "type": "command", "command": VD_COMMAND }]
    })
}

fn has_vd_hook(matcher_groups: &[Value]) -> bool {
    matcher_groups.iter().any(|group| {
        group
            .get("hooks")
            .and_then(|h| h.as_array())
            .is_some_and(|hooks| {
                hooks.iter().any(|hook| {
                    hook.get("command")
                        .and_then(|c| c.as_str())
                        .is_some_and(|cmd| cmd == VD_COMMAND || cmd == LEGACY_COMMAND)
                })
            })
    })
}

fn upgrade_legacy_hooks(matcher_groups: &mut [Value]) -> bool {
    let mut upgraded = false;
    for group in matcher_groups.iter_mut() {
        if let Some(hooks) = group.get_mut("hooks").and_then(|h| h.as_array_mut()) {
            for hook in hooks.iter_mut() {
                if hook.get("command").and_then(|c| c.as_str()) == Some(LEGACY_COMMAND) {
                    hook.as_object_mut()
                        .unwrap()
                        .insert("command".to_string(), Value::String(VD_COMMAND.to_string()));
                    upgraded = true;
                }
            }
        }
    }
    upgraded
}

pub fn install(settings_path: &Path) -> Result<InstallReport> {
    let mut settings = load_settings(settings_path)?;
    let mut report = InstallReport {
        data_dir_created: false,
        settings_created: !settings_path.exists(),
        hooks_added: Vec::new(),
        hooks_already_present: Vec::new(),
        hooks_upgraded: Vec::new(),
    };

    let hooks = settings
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| color_eyre::eyre::eyre!("\"hooks\" key is not a JSON object"))?;

    for &event in REQUIRED_EVENTS {
        let event_array = hooks
            .entry(event)
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .ok_or_else(|| color_eyre::eyre::eyre!("hooks.{event} is not an array"))?;

        if has_vd_hook(event_array) {
            if upgrade_legacy_hooks(event_array) {
                report.hooks_upgraded.push(event.to_string());
            } else {
                report.hooks_already_present.push(event.to_string());
            }
        } else {
            event_array.push(build_vd_matcher_group());
            report.hooks_added.push(event.to_string());
        }
    }

    save_settings(settings_path, &settings)?;
    Ok(report)
}

pub fn uninstall(settings_path: &Path) -> Result<UninstallReport> {
    if !settings_path.exists() {
        return Ok(UninstallReport {
            hooks_removed: Vec::new(),
            hooks_not_found: REQUIRED_EVENTS.iter().map(|e| e.to_string()).collect(),
        });
    }

    let mut settings = load_settings(settings_path)?;
    let mut report = UninstallReport {
        hooks_removed: Vec::new(),
        hooks_not_found: Vec::new(),
    };

    if let Some(hooks) = settings.get_mut("hooks").and_then(|h| h.as_object_mut()) {
        for &event in REQUIRED_EVENTS {
            if let Some(event_array) = hooks.get_mut(event).and_then(|a| a.as_array_mut()) {
                let before_len = event_array.len();
                event_array.retain(|group| {
                    !group
                        .get("hooks")
                        .and_then(|h| h.as_array())
                        .is_some_and(|inner| {
                            inner.iter().any(|hook| {
                                hook.get("command")
                                    .and_then(|c| c.as_str())
                                    .is_some_and(|cmd| cmd == VD_COMMAND || cmd == LEGACY_COMMAND)
                            })
                        })
                });
                if event_array.len() < before_len {
                    report.hooks_removed.push(event.to_string());
                } else {
                    report.hooks_not_found.push(event.to_string());
                }
                if event_array.is_empty() {
                    hooks.remove(event);
                }
            } else {
                report.hooks_not_found.push(event.to_string());
            }
        }

        if hooks.is_empty() {
            settings.remove("hooks");
        }
    } else {
        report.hooks_not_found = REQUIRED_EVENTS.iter().map(|e| e.to_string()).collect();
    }

    save_settings(settings_path, &settings)?;
    Ok(report)
}

pub fn run_install() -> Result<()> {
    let data_dir = vd_data_dir()?;
    let data_dir_created = !data_dir.exists();
    if data_dir_created {
        fs::create_dir_all(&data_dir)?;
    }

    let settings_path = claude_settings_path()?;
    let mut report = install(&settings_path)?;
    report.data_dir_created = data_dir_created;

    println!("vd install complete:");
    if report.data_dir_created {
        println!("  Created {}", data_dir.display());
    }
    if report.settings_created {
        println!("  Created {}", settings_path.display());
    }
    for event in &report.hooks_added {
        println!("  + {event} hook added");
    }
    for event in &report.hooks_upgraded {
        println!("  ↑ {event} hook upgraded (van-damme → vd)");
    }
    for event in &report.hooks_already_present {
        println!("  ✓ {event} hook already present");
    }
    Ok(())
}

pub fn run_uninstall() -> Result<()> {
    let settings_path = claude_settings_path()?;
    let report = uninstall(&settings_path)?;

    println!("vd uninstall complete:");
    for event in &report.hooks_removed {
        println!("  - {event} hook removed");
    }
    for event in &report.hooks_not_found {
        println!("  ~ {event} hook was not installed");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_settings() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        (tmp, path)
    }

    #[test]
    fn test_install_empty_settings() {
        let (_tmp, path) = temp_settings();
        let report = install(&path).unwrap();
        assert_eq!(report.hooks_added.len(), 4);
        assert!(report.hooks_already_present.is_empty());

        let settings = load_settings(&path).unwrap();
        let hooks = settings["hooks"].as_object().unwrap();
        assert!(hooks.contains_key("SessionStart"));
        assert!(hooks.contains_key("Stop"));
        assert!(hooks.contains_key("UserPromptSubmit"));
        assert!(hooks.contains_key("PermissionRequest"));
    }

    #[test]
    fn test_install_preserves_other_settings() {
        let (_tmp, path) = temp_settings();
        let initial = json!({
            "model": "sonnet",
            "permissions": { "allow": ["Bash"] }
        });
        fs::write(&path, serde_json::to_string_pretty(&initial).unwrap()).unwrap();

        install(&path).unwrap();

        let settings = load_settings(&path).unwrap();
        assert_eq!(settings["model"], "sonnet");
        assert_eq!(settings["permissions"]["allow"][0], "Bash");
        assert!(settings.contains_key("hooks"));
    }

    #[test]
    fn test_install_preserves_other_hooks() {
        let (_tmp, path) = temp_settings();
        let initial = json!({
            "hooks": {
                "PreToolUse": [{ "matcher": "Bash", "hooks": [{ "type": "command", "command": "other-tool" }] }]
            }
        });
        fs::write(&path, serde_json::to_string_pretty(&initial).unwrap()).unwrap();

        install(&path).unwrap();

        let settings = load_settings(&path).unwrap();
        let hooks = settings["hooks"].as_object().unwrap();
        let pre_tool = hooks["PreToolUse"].as_array().unwrap();
        assert_eq!(pre_tool.len(), 1);
        assert_eq!(pre_tool[0]["hooks"][0]["command"], "other-tool");
    }

    #[test]
    fn test_install_idempotent() {
        let (_tmp, path) = temp_settings();
        install(&path).unwrap();
        let report = install(&path).unwrap();

        assert!(report.hooks_added.is_empty());
        assert_eq!(report.hooks_already_present.len(), 4);

        let settings = load_settings(&path).unwrap();
        let hooks = settings["hooks"].as_object().unwrap();
        for &event in REQUIRED_EVENTS {
            assert_eq!(hooks[event].as_array().unwrap().len(), 1);
        }
    }

    #[test]
    fn test_install_upgrades_legacy_command() {
        let (_tmp, path) = temp_settings();
        let initial = json!({
            "hooks": {
                "SessionStart": [{ "matcher": "", "hooks": [{ "type": "command", "command": "van-damme process-hook" }] }]
            }
        });
        fs::write(&path, serde_json::to_string_pretty(&initial).unwrap()).unwrap();

        let report = install(&path).unwrap();

        assert!(report.hooks_upgraded.contains(&"SessionStart".to_string()));
        let settings = load_settings(&path).unwrap();
        let cmd = settings["hooks"]["SessionStart"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap();
        assert_eq!(cmd, VD_COMMAND);
    }

    #[test]
    fn test_uninstall_removes_vd_hooks() {
        let (_tmp, path) = temp_settings();
        install(&path).unwrap();
        let report = uninstall(&path).unwrap();

        assert_eq!(report.hooks_removed.len(), 4);
        let settings = load_settings(&path).unwrap();
        assert!(!settings.contains_key("hooks"));
    }

    #[test]
    fn test_uninstall_preserves_other_hooks() {
        let (_tmp, path) = temp_settings();
        let initial = json!({
            "hooks": {
                "SessionStart": [
                    { "matcher": "", "hooks": [{ "type": "command", "command": VD_COMMAND }] },
                    { "matcher": "", "hooks": [{ "type": "command", "command": "other-tool" }] }
                ]
            }
        });
        fs::write(&path, serde_json::to_string_pretty(&initial).unwrap()).unwrap();

        uninstall(&path).unwrap();

        let settings = load_settings(&path).unwrap();
        let hooks = settings["hooks"].as_object().unwrap();
        let session_start = hooks["SessionStart"].as_array().unwrap();
        assert_eq!(session_start.len(), 1);
        assert_eq!(session_start[0]["hooks"][0]["command"], "other-tool");
    }

    #[test]
    fn test_uninstall_missing_file() {
        let (_tmp, path) = temp_settings();
        let report = uninstall(&path).unwrap();
        assert_eq!(report.hooks_not_found.len(), 4);
    }

    #[test]
    fn test_has_vd_hook_detects_current() {
        let groups = vec![json!({
            "matcher": "",
            "hooks": [{ "type": "command", "command": VD_COMMAND }]
        })];
        assert!(has_vd_hook(&groups));
    }

    #[test]
    fn test_has_vd_hook_detects_legacy() {
        let groups = vec![json!({
            "matcher": "",
            "hooks": [{ "type": "command", "command": LEGACY_COMMAND }]
        })];
        assert!(has_vd_hook(&groups));
    }

    #[test]
    fn test_has_vd_hook_false_for_other() {
        let groups = vec![json!({
            "matcher": "",
            "hooks": [{ "type": "command", "command": "other-tool" }]
        })];
        assert!(!has_vd_hook(&groups));
    }
}
