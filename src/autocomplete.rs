use std::path::Path;

use crate::git;

pub struct DirCompleter;

impl DirCompleter {
    /// Returns the completed path on Tab press, or None if no progress can be made.
    pub fn complete(&self, input: &str) -> Option<String> {
        complete_path(input).map(|(completed, _)| completed)
    }

    /// Returns the ghost-text suffix to display after the cursor, or None.
    pub fn suggest(&self, input: &str) -> Option<String> {
        complete_path(input).and_then(|(completed, _)| {
            let suffix = completed.strip_prefix(input)?;
            if suffix.is_empty() { None } else { Some(suffix.to_string()) }
        })
    }
}

pub struct BranchCompleter;

impl BranchCompleter {
    /// Returns the longest-common-prefix completion, or None if no progress.
    pub fn complete(&self, prefix: &str, dir: &str) -> Option<String> {
        if dir.is_empty() {
            return None;
        }
        let branches = git::get_local_branches(dir);
        let prefix_lower = prefix.to_lowercase();
        let mut matches: Vec<&String> = branches
            .iter()
            .filter(|b| b.to_lowercase().starts_with(&prefix_lower) && b.as_str() != prefix)
            .collect();
        matches.sort();
        if matches.is_empty() {
            return None;
        }
        let common = longest_common_prefix(&matches.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        if common == prefix {
            None
        } else {
            Some(common)
        }
    }

    /// Returns the ghost-text suffix of the first matching branch, or None.
    pub fn suggest(&self, prefix: &str, dir: &str) -> Option<String> {
        if prefix.is_empty() || dir.is_empty() {
            return None;
        }
        let branches = git::get_local_branches(dir);
        branches
            .iter()
            .find(|b| b.starts_with(prefix) && b.as_str() != prefix)
            .and_then(|b| b.strip_prefix(prefix))
            .map(|s| s.to_string())
    }
}

/// Compute directory tab-completion for a given input path.
/// Returns (completed_path, optional_ghost_suggestion) or None if no matches.
fn complete_path(input: &str) -> Option<(String, Option<String>)> {
    if input.is_empty() {
        return None;
    }

    let path = Path::new(input);

    let (parent, prefix) = if input.ends_with('/') && path.is_dir() {
        (path.to_path_buf(), "")
    } else {
        let parent = path.parent()?;
        let file_name = path.file_name()?.to_str()?;
        (parent.to_path_buf(), file_name)
    };

    let entries = std::fs::read_dir(&parent).ok()?;
    let mut matches: Vec<String> = Vec::new();

    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with(prefix) && entry.path().is_dir() {
            matches.push(name_str.to_string());
        }
    }

    if matches.is_empty() {
        return None;
    }

    matches.sort();

    let common = longest_common_prefix(&matches);

    let completed = if input.ends_with('/') || prefix.is_empty() {
        format!("{}{}", parent.display(), std::path::MAIN_SEPARATOR)
            + &common
            + if matches.len() == 1 {
                std::str::from_utf8(&[std::path::MAIN_SEPARATOR as u8]).unwrap_or("/")
            } else {
                ""
            }
    } else {
        let parent_str = parent.display().to_string();
        let sep = if parent_str.ends_with('/') { "" } else { "/" };
        format!(
            "{}{}{}{}",
            parent_str,
            sep,
            common,
            if matches.len() == 1 { "/" } else { "" }
        )
    };

    let suggestion = if matches.len() > 1 {
        Some(matches[0].clone())
    } else {
        None
    };

    if completed == input {
        if matches.len() > 1 {
            return Some((completed, Some(matches[0].clone())));
        }
        return None;
    }

    Some((completed, suggestion))
}

pub fn longest_common_prefix(strings: &[String]) -> String {
    if strings.is_empty() {
        return String::new();
    }
    let first = &strings[0];
    let mut len = first.len();
    for s in &strings[1..] {
        len = len.min(s.len());
        for (i, (a, b)) in first.chars().zip(s.chars()).enumerate() {
            if a != b {
                len = len.min(i);
                break;
            }
        }
    }
    first[..len].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_longest_common_prefix_empty() {
        assert_eq!(longest_common_prefix(&[]), "");
    }

    #[test]
    fn test_longest_common_prefix_single() {
        assert_eq!(longest_common_prefix(&["hello".to_string()]), "hello");
    }

    #[test]
    fn test_longest_common_prefix_common() {
        let v = vec!["feature-auth".to_string(), "feature-api".to_string()];
        assert_eq!(longest_common_prefix(&v), "feature-a");
    }

    #[test]
    fn test_longest_common_prefix_no_common() {
        let v = vec!["abc".to_string(), "xyz".to_string()];
        assert_eq!(longest_common_prefix(&v), "");
    }

    #[test]
    fn test_dir_completer_suggest_empty_input() {
        let c = DirCompleter;
        assert_eq!(c.suggest(""), None);
    }

    #[test]
    fn test_branch_completer_suggest_empty_prefix() {
        let c = BranchCompleter;
        assert_eq!(c.suggest("", "/tmp"), None);
    }

    #[test]
    fn test_branch_completer_suggest_empty_dir() {
        let c = BranchCompleter;
        assert_eq!(c.suggest("feat", ""), None);
    }

    #[test]
    fn test_branch_completer_complete_empty_dir() {
        let c = BranchCompleter;
        assert_eq!(c.complete("feat", ""), None);
    }
}
