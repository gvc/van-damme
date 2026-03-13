use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Position},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use std::path::Path;
use tui_input::Input;
use tui_input::backend::crossterm::EventHandler;

/// Compute directory tab-completion for a given input path.
/// Returns the completed path if there's a unique or common-prefix completion,
/// along with the full suggestion text (for ghost display).
/// Returns None if no completions are found.
pub fn complete_path(input: &str) -> Option<(String, Option<String>)> {
    if input.is_empty() {
        return None;
    }

    let path = Path::new(input);

    // If input ends with '/' and is a directory, list its children
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
        if name_str.starts_with(prefix) {
            // Only complete to directories
            if entry.path().is_dir() {
                matches.push(name_str.to_string());
            }
        }
    }

    if matches.is_empty() {
        return None;
    }

    matches.sort();

    // Find longest common prefix among matches
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

    // Ghost suggestion: show the first match fully if there are multiple
    let suggestion = if matches.len() > 1 {
        Some(matches[0].clone())
    } else {
        None
    };

    if completed == input {
        // No progress made — show first match as suggestion
        if matches.len() > 1 {
            return Some((completed, Some(matches[0].clone())));
        }
        return None;
    }

    Some((completed, suggestion))
}

fn longest_common_prefix(strings: &[String]) -> String {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputField {
    Title,
    Directory,
    Prompt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    None,
    Quit,
    Submit {
        title: String,
        directory: String,
        prompt: Option<String>,
    },
}

#[derive(Debug)]
pub struct App {
    pub running: bool,
    pub focused_field: InputField,
    pub title_input: Input,
    pub dir_input: Input,
    pub prompt_input: Input,
    pub dir_suggestion: Option<String>,
    pub error_message: Option<String>,
}

impl App {
    pub fn new() -> Self {
        let cwd = std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        Self {
            running: true,
            focused_field: InputField::Title,
            title_input: Input::default(),
            dir_input: Input::new(cwd),
            prompt_input: Input::default(),
            dir_suggestion: None,
            error_message: None,
        }
    }

    pub fn quit(&mut self) {
        self.running = false;
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Esc => {
                self.quit();
                Action::Quit
            }
            KeyCode::Tab | KeyCode::Down => {
                self.next_field();
                Action::None
            }
            KeyCode::BackTab | KeyCode::Up => {
                self.prev_field();
                Action::None
            }
            KeyCode::Right
                if self.focused_field == InputField::Directory
                    && self.dir_suggestion.is_some()
                    && self.cursor_at_end() =>
            {
                self.complete_directory();
                Action::None
            }
            KeyCode::Enter => self.handle_enter(),
            _ => {
                // Forward to focused input
                match self.focused_field {
                    InputField::Title => {
                        self.title_input
                            .handle_event(&crossterm::event::Event::Key(key));
                    }
                    InputField::Directory => {
                        self.dir_input
                            .handle_event(&crossterm::event::Event::Key(key));
                        self.update_dir_suggestion();
                    }
                    InputField::Prompt => {
                        self.prompt_input
                            .handle_event(&crossterm::event::Event::Key(key));
                    }
                }
                self.error_message = None;
                Action::None
            }
        }
    }

    fn cursor_at_end(&self) -> bool {
        self.dir_input.visual_cursor() >= self.dir_input.value().len()
    }

    fn complete_directory(&mut self) {
        let current = self.dir_input.value().to_string();
        if let Some((completed, _suggestion)) = complete_path(&current) {
            self.dir_input = Input::new(completed.clone());
            // Move cursor to end
            let len = completed.len();
            for _ in 0..len {
                self.dir_input
                    .handle_event(&crossterm::event::Event::Key(KeyEvent::new(
                        KeyCode::Right,
                        crossterm::event::KeyModifiers::NONE,
                    )));
            }
            self.update_dir_suggestion();
        }
    }

    fn update_dir_suggestion(&mut self) {
        let current = self.dir_input.value().to_string();
        self.dir_suggestion = complete_path(&current).and_then(|(completed, _)| {
            let suffix = completed.strip_prefix(&current)?;
            if suffix.is_empty() {
                None
            } else {
                Some(suffix.to_string())
            }
        });
    }

    fn next_field(&mut self) {
        self.focused_field = match self.focused_field {
            InputField::Title => InputField::Directory,
            InputField::Directory => InputField::Prompt,
            InputField::Prompt => InputField::Title,
        };
    }

    fn prev_field(&mut self) {
        self.focused_field = match self.focused_field {
            InputField::Title => InputField::Prompt,
            InputField::Directory => InputField::Title,
            InputField::Prompt => InputField::Directory,
        };
    }

    fn handle_enter(&mut self) -> Action {
        let title = self.title_input.value().trim().to_string();
        let directory = self.dir_input.value().trim().to_string();

        if title.is_empty() {
            self.error_message = Some("Title cannot be empty".to_string());
            self.focused_field = InputField::Title;
            return Action::None;
        }

        if directory.is_empty() {
            self.error_message = Some("Directory cannot be empty".to_string());
            return Action::None;
        }

        if !Path::new(&directory).is_dir() {
            self.error_message = Some(format!("Directory does not exist: {directory}"));
            return Action::None;
        }

        let prompt_raw = self.prompt_input.value().trim().to_string();
        let prompt = if prompt_raw.is_empty() {
            None
        } else {
            Some(prompt_raw)
        };

        Action::Submit {
            title,
            directory,
            prompt,
        }
    }

    pub fn draw(&self, frame: &mut Frame) {
        let area = frame.area();

        // Centered form: 60 wide, 16 tall
        let form_width = 60u16.min(area.width.saturating_sub(2));
        let form_height = 16u16.min(area.height.saturating_sub(2));

        let vertical = Layout::vertical([Constraint::Length(form_height)])
            .flex(Flex::Center)
            .split(area);
        let horizontal = Layout::horizontal([Constraint::Length(form_width)])
            .flex(Flex::Center)
            .split(vertical[0]);
        let form_area = horizontal[0];

        // Clear area behind form
        frame.render_widget(Clear, form_area);

        let outer_block = Block::default()
            .title(" New Task ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));
        let inner = outer_block.inner(form_area);
        frame.render_widget(outer_block, form_area);

        // Layout inside form: label+input for title, directory, prompt, hint, error
        let chunks = Layout::vertical([
            Constraint::Length(1), // Title label
            Constraint::Length(3), // Title input
            Constraint::Length(1), // Directory label
            Constraint::Length(3), // Directory input
            Constraint::Length(1), // Prompt label
            Constraint::Length(3), // Prompt input
            Constraint::Min(1),    // Hints + error
        ])
        .split(inner);

        // Title label
        let title_label = Paragraph::new("Title:");
        frame.render_widget(title_label, chunks[0]);

        // Title input
        let title_border_color = if self.focused_field == InputField::Title {
            Color::Yellow
        } else {
            Color::DarkGray
        };
        let title_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(title_border_color));
        let title_inner = title_block.inner(chunks[1]);
        let title_para = Paragraph::new(self.title_input.value()).block(title_block);
        frame.render_widget(title_para, chunks[1]);

        // Directory label
        let dir_label = Paragraph::new("Directory:");
        frame.render_widget(dir_label, chunks[2]);

        // Directory input
        let dir_border_color = if self.focused_field == InputField::Directory {
            Color::Yellow
        } else {
            Color::DarkGray
        };
        let dir_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(dir_border_color));
        let dir_inner = dir_block.inner(chunks[3]);
        let dir_value = self.dir_input.value();
        let dir_line = if let Some(ref suggestion) = self.dir_suggestion {
            Line::from(vec![
                Span::raw(dir_value.to_string()),
                Span::styled(suggestion.as_str(), Style::default().fg(Color::DarkGray)),
            ])
        } else {
            Line::from(dir_value.to_string())
        };
        let dir_para = Paragraph::new(dir_line).block(dir_block);
        frame.render_widget(dir_para, chunks[3]);

        // Prompt label
        let prompt_label = Paragraph::new("Initial prompt (optional):");
        frame.render_widget(prompt_label, chunks[4]);

        // Prompt input
        let prompt_border_color = if self.focused_field == InputField::Prompt {
            Color::Yellow
        } else {
            Color::DarkGray
        };
        let prompt_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(prompt_border_color));
        let prompt_inner = prompt_block.inner(chunks[5]);
        let prompt_para = Paragraph::new(self.prompt_input.value()).block(prompt_block);
        frame.render_widget(prompt_para, chunks[5]);

        // Hints + error
        let hint_text =
            if self.focused_field == InputField::Directory && self.dir_suggestion.is_some() {
                "→: complete path  |  Tab: next field  |  Enter: submit  |  Esc: quit"
            } else {
                "Tab: next field  |  Enter: submit  |  Esc: quit"
            };
        let mut hint_lines: Vec<Line> = vec![Line::from(Span::raw(hint_text).dark_gray())];

        if let Some(ref err) = self.error_message {
            hint_lines.push(Line::from(Span::raw(err.as_str()).fg(Color::Red)));
        }

        let hints = Paragraph::new(hint_lines);
        frame.render_widget(hints, chunks[6]);

        // Place cursor in focused input
        let (cursor_input, cursor_area) = match self.focused_field {
            InputField::Title => (&self.title_input, title_inner),
            InputField::Directory => (&self.dir_input, dir_inner),
            InputField::Prompt => (&self.prompt_input, prompt_inner),
        };
        let cursor_x = cursor_area.x + cursor_input.visual_cursor() as u16;
        let cursor_y = cursor_area.y;
        frame.set_cursor_position(Position::new(cursor_x, cursor_y));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn test_new_app_is_running() {
        let app = App::new();
        assert!(app.running);
        assert_eq!(app.focused_field, InputField::Title);
    }

    #[test]
    fn test_esc_returns_quit() {
        let mut app = App::new();
        let action = app.handle_key(key(KeyCode::Esc));
        assert_eq!(action, Action::Quit);
        assert!(!app.running);
    }

    #[test]
    fn test_tab_cycles_all_fields() {
        let mut app = App::new();
        assert_eq!(app.focused_field, InputField::Title);

        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.focused_field, InputField::Directory);

        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.focused_field, InputField::Prompt);

        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.focused_field, InputField::Title);
    }

    #[test]
    fn test_enter_with_empty_title_sets_error() {
        let mut app = App::new();
        let action = app.handle_key(key(KeyCode::Enter));
        assert_eq!(action, Action::None);
        assert!(app.error_message.is_some());
        assert!(app.error_message.unwrap().contains("Title"));
    }

    #[test]
    fn test_enter_on_title_submits_directly() {
        let mut app = App::new();
        // Default dir_input is CWD which should exist
        for ch in "my task".chars() {
            app.handle_key(key(KeyCode::Char(ch)));
        }
        let action = app.handle_key(key(KeyCode::Enter));
        assert!(matches!(action, Action::Submit { .. }));
    }

    #[test]
    fn test_submit_with_custom_directory() {
        let mut app = App::new();
        for ch in "my task".chars() {
            app.handle_key(key(KeyCode::Char(ch)));
        }
        // Move to directory and change it
        app.handle_key(key(KeyCode::Tab));
        while !app.dir_input.value().is_empty() {
            app.handle_key(key(KeyCode::Backspace));
        }
        for ch in "/tmp".chars() {
            app.handle_key(key(KeyCode::Char(ch)));
        }
        let action = app.handle_key(key(KeyCode::Enter));
        assert_eq!(
            action,
            Action::Submit {
                title: "my task".to_string(),
                directory: "/tmp".to_string(),
                prompt: None,
            }
        );
    }

    #[test]
    fn test_submit_with_nonexistent_directory() {
        let mut app = App::new();
        for ch in "my task".chars() {
            app.handle_key(key(KeyCode::Char(ch)));
        }
        app.handle_key(key(KeyCode::Tab));
        while !app.dir_input.value().is_empty() {
            app.handle_key(key(KeyCode::Backspace));
        }
        for ch in "/nonexistent/path/12345".chars() {
            app.handle_key(key(KeyCode::Char(ch)));
        }
        let action = app.handle_key(key(KeyCode::Enter));
        assert_eq!(action, Action::None);
        assert!(app.error_message.is_some());
        assert!(app.error_message.unwrap().contains("does not exist"));
    }

    #[test]
    fn test_typing_clears_error() {
        let mut app = App::new();
        app.error_message = Some("some error".to_string());
        app.handle_key(key(KeyCode::Char('a')));
        assert!(app.error_message.is_none());
    }

    #[test]
    fn test_up_down_cycles_focus() {
        let mut app = App::new();
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.focused_field, InputField::Directory);
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.focused_field, InputField::Prompt);
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.focused_field, InputField::Directory);
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.focused_field, InputField::Title);
    }

    #[test]
    fn test_submit_with_prompt() {
        let mut app = App::new();
        for ch in "my task".chars() {
            app.handle_key(key(KeyCode::Char(ch)));
        }
        // Skip to prompt field
        app.handle_key(key(KeyCode::Tab)); // -> Directory
        app.handle_key(key(KeyCode::Tab)); // -> Prompt
        for ch in "fix the bug".chars() {
            app.handle_key(key(KeyCode::Char(ch)));
        }
        let action = app.handle_key(key(KeyCode::Enter));
        assert_eq!(
            action,
            Action::Submit {
                title: "my task".to_string(),
                directory: std::env::current_dir()
                    .unwrap()
                    .to_string_lossy()
                    .to_string(),
                prompt: Some("fix the bug".to_string()),
            }
        );
    }

    #[test]
    fn test_submit_with_empty_prompt_is_none() {
        let mut app = App::new();
        for ch in "my task".chars() {
            app.handle_key(key(KeyCode::Char(ch)));
        }
        let action = app.handle_key(key(KeyCode::Enter));
        match action {
            Action::Submit { prompt, .. } => assert!(prompt.is_none()),
            _ => panic!("Expected Submit action"),
        }
    }

    #[test]
    fn test_complete_path_returns_none_for_empty() {
        assert!(complete_path("").is_none());
    }

    #[test]
    fn test_complete_path_completes_tmp() {
        // /tmp should exist on all systems
        let result = complete_path("/tm");
        assert!(result.is_some());
        let (completed, _) = result.unwrap();
        assert!(completed.starts_with("/tmp"));
    }

    #[test]
    fn test_complete_path_trailing_slash_lists_children() {
        // /tmp/ should list children of /tmp
        let result = complete_path("/tmp/");
        // May or may not have completions depending on /tmp contents,
        // but it should not panic
        let _ = result;
    }

    #[test]
    fn test_complete_path_nonexistent_returns_none() {
        assert!(complete_path("/nonexistent_path_xyz_12345/abc").is_none());
    }

    #[test]
    fn test_longest_common_prefix_single() {
        assert_eq!(longest_common_prefix(&["hello".to_string()]), "hello");
    }

    #[test]
    fn test_longest_common_prefix_multiple() {
        assert_eq!(
            longest_common_prefix(&["hello".to_string(), "help".to_string(), "hero".to_string()]),
            "he"
        );
    }

    #[test]
    fn test_longest_common_prefix_empty_list() {
        let empty: Vec<String> = vec![];
        assert_eq!(longest_common_prefix(&empty), "");
    }

    #[test]
    fn test_longest_common_prefix_identical() {
        assert_eq!(
            longest_common_prefix(&["abc".to_string(), "abc".to_string()]),
            "abc"
        );
    }

    #[test]
    fn test_right_arrow_on_directory_triggers_completion() {
        let mut app = App::new();
        app.focused_field = InputField::Directory;
        // Set input to /tm which should complete to /tmp
        app.dir_input = Input::new("/tm".to_string());
        // Move cursor to end
        for _ in 0..3 {
            app.dir_input
                .handle_event(&crossterm::event::Event::Key(key(KeyCode::Right)));
        }
        app.update_dir_suggestion();
        assert!(app.dir_suggestion.is_some());
        app.handle_key(key(KeyCode::Right));
        assert!(app.dir_input.value().starts_with("/tmp"));
        // Should still be on Directory field
        assert_eq!(app.focused_field, InputField::Directory);
    }

    #[test]
    fn test_right_arrow_without_suggestion_moves_cursor() {
        let mut app = App::new();
        app.focused_field = InputField::Directory;
        app.dir_input = Input::new("/tmp".to_string());
        // Move cursor to position 0
        for _ in 0..10 {
            app.dir_input
                .handle_event(&crossterm::event::Event::Key(key(KeyCode::Home)));
        }
        app.update_dir_suggestion();
        // Right arrow should move cursor, not complete (cursor not at end)
        let cursor_before = app.dir_input.visual_cursor();
        app.handle_key(key(KeyCode::Right));
        // Cursor should have moved (passed through to input handler)
        assert!(app.dir_input.visual_cursor() > cursor_before || app.dir_suggestion.is_none());
    }

    #[test]
    fn test_tab_on_directory_moves_to_prompt() {
        let mut app = App::new();
        app.focused_field = InputField::Directory;
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.focused_field, InputField::Prompt);
    }
}
