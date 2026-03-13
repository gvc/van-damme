use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Flex, Layout, Position},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use std::path::Path;
use tui_input::Input;
use tui_input::backend::crossterm::EventHandler;

use crate::theme;

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
    pub recent_dirs: Vec<String>,
    pub recent_dir_selected: Option<usize>,
    pub show_recent_dirs: bool,
}

impl App {
    #[cfg(test)]
    pub fn new() -> Self {
        Self::with_recent_dirs(Vec::new())
    }

    pub fn with_recent_dirs(recent_dirs: Vec<String>) -> Self {
        let default_dir = recent_dirs.first().cloned().unwrap_or_else(|| {
            std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default()
        });

        Self {
            running: true,
            focused_field: InputField::Title,
            title_input: Input::default(),
            dir_input: Input::new(default_dir),
            prompt_input: Input::default(),
            dir_suggestion: None,
            error_message: None,
            recent_dirs,
            recent_dir_selected: None,
            show_recent_dirs: false,
        }
    }

    pub fn quit(&mut self) {
        self.running = false;
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Action {
        // Handle recent dirs dropdown navigation
        if self.show_recent_dirs {
            return self.handle_recent_dirs_key(key);
        }

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
                // Ctrl+D toggles recent dirs when on directory field
                if self.focused_field == InputField::Directory
                    && key.code == KeyCode::Char('d')
                    && key
                        .modifiers
                        .contains(crossterm::event::KeyModifiers::CONTROL)
                    && !self.recent_dirs.is_empty()
                {
                    self.show_recent_dirs = true;
                    self.recent_dir_selected = Some(0);
                    return Action::None;
                }

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

    fn handle_recent_dirs_key(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Esc => {
                self.show_recent_dirs = false;
                self.recent_dir_selected = None;
                Action::None
            }
            KeyCode::Up | KeyCode::BackTab => {
                if let Some(i) = self.recent_dir_selected {
                    if i > 0 {
                        self.recent_dir_selected = Some(i - 1);
                    } else {
                        self.recent_dir_selected = Some(self.recent_dirs.len() - 1);
                    }
                }
                Action::None
            }
            KeyCode::Down | KeyCode::Tab => {
                if let Some(i) = self.recent_dir_selected {
                    if i < self.recent_dirs.len() - 1 {
                        self.recent_dir_selected = Some(i + 1);
                    } else {
                        self.recent_dir_selected = Some(0);
                    }
                }
                Action::None
            }
            KeyCode::Enter => {
                if let Some(i) = self.recent_dir_selected
                    && let Some(dir) = self.recent_dirs.get(i)
                {
                    let dir = dir.clone();
                    self.dir_input = Input::new(dir.clone());
                    // Move cursor to end
                    for _ in 0..dir.len() {
                        self.dir_input
                            .handle_event(&crossterm::event::Event::Key(KeyEvent::new(
                                KeyCode::Right,
                                crossterm::event::KeyModifiers::NONE,
                            )));
                    }
                    self.update_dir_suggestion();
                }
                self.show_recent_dirs = false;
                self.recent_dir_selected = None;
                Action::None
            }
            _ => Action::None,
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

        // Centered form: 60 wide, dynamically sized vertically
        let form_width = 60u16.min(area.width.saturating_sub(2));

        // Calculate prompt input height based on text wrapping
        // Inner width = form_width - 2 (outer border) - 2 (input border)
        let prompt_inner_width = form_width.saturating_sub(4) as usize;
        let prompt_lines = if prompt_inner_width == 0 {
            1
        } else {
            let text_len = self.prompt_input.value().len();
            ((text_len as f64 / prompt_inner_width as f64).ceil() as u16).max(1)
        };
        // Prompt input box height = lines + 2 (borders), capped to leave room
        let max_prompt_height = area.height.saturating_sub(16); // leave room for other fields
        let prompt_box_height = (prompt_lines + 2).min(max_prompt_height).max(3);

        // 2 (outer border) + 1+3+1+3+1 (labels+inputs) + prompt_box_height + 1 (hints)
        let form_height = (12 + prompt_box_height).min(area.height.saturating_sub(2));
        // +1 for error line below the box
        let total_height = form_height + 1;

        let vertical = Layout::vertical([Constraint::Length(total_height)])
            .flex(Flex::Center)
            .split(area);
        let horizontal = Layout::horizontal([Constraint::Length(form_width)])
            .flex(Flex::Center)
            .split(vertical[0]);
        let outer_area = horizontal[0];

        // Split into form box and error line below
        let outer_chunks = Layout::vertical([
            Constraint::Length(form_height),
            Constraint::Length(1),
        ])
        .split(outer_area);
        let form_area = outer_chunks[0];
        let error_area = outer_chunks[1];

        // Clear area behind form and fill with background
        frame.render_widget(Clear, form_area);
        frame.render_widget(
            Block::default().style(Style::default().bg(theme::BG)),
            form_area,
        );

        let outer_block = Block::default()
            .title(" New Task ")
            .title_style(Style::default().fg(theme::ORANGE_BRIGHT))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::ORANGE))
            .style(Style::default().bg(theme::BG));
        let inner = outer_block.inner(form_area);
        frame.render_widget(outer_block, form_area);

        // Layout inside form: label+input for title, directory, prompt, hint
        let chunks = Layout::vertical([
            Constraint::Length(1),                 // Title label
            Constraint::Length(3),                 // Title input
            Constraint::Length(1),                 // Directory label
            Constraint::Length(3),                 // Directory input
            Constraint::Length(1),                 // Prompt label
            Constraint::Length(prompt_box_height), // Prompt input (grows with text)
            Constraint::Min(1),                    // Hints
        ])
        .split(inner);

        // Title label
        let title_label =
            Paragraph::new("Title:").style(Style::default().fg(theme::TEXT).bg(theme::BG));
        frame.render_widget(title_label, chunks[0]);

        // Title input
        let title_border_color = if self.focused_field == InputField::Title {
            theme::ORANGE_BRIGHT
        } else {
            theme::GRAY
        };
        let title_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(title_border_color))
            .style(Style::default().bg(theme::BG));
        let title_inner = title_block.inner(chunks[1]);
        let title_para = Paragraph::new(self.title_input.value())
            .style(Style::default().fg(theme::TEXT))
            .block(title_block);
        frame.render_widget(title_para, chunks[1]);

        // Directory label
        let dir_label =
            Paragraph::new("Directory:").style(Style::default().fg(theme::TEXT).bg(theme::BG));
        frame.render_widget(dir_label, chunks[2]);

        // Directory input
        let dir_border_color = if self.focused_field == InputField::Directory {
            theme::ORANGE_BRIGHT
        } else {
            theme::GRAY
        };
        let dir_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(dir_border_color))
            .style(Style::default().bg(theme::BG));
        let dir_inner = dir_block.inner(chunks[3]);
        let dir_value = self.dir_input.value();
        let dir_line = if let Some(ref suggestion) = self.dir_suggestion {
            Line::from(vec![
                Span::styled(dir_value.to_string(), Style::default().fg(theme::TEXT)),
                Span::styled(suggestion.as_str(), Style::default().fg(theme::BLUE)),
            ])
        } else {
            Line::from(Span::styled(
                dir_value.to_string(),
                Style::default().fg(theme::TEXT),
            ))
        };
        let dir_para = Paragraph::new(dir_line).block(dir_block);
        frame.render_widget(dir_para, chunks[3]);

        // Prompt label
        let prompt_label = Paragraph::new("Initial prompt (optional):")
            .style(Style::default().fg(theme::TEXT).bg(theme::BG));
        frame.render_widget(prompt_label, chunks[4]);

        // Prompt input
        let prompt_border_color = if self.focused_field == InputField::Prompt {
            theme::ORANGE_BRIGHT
        } else {
            theme::GRAY
        };
        let prompt_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(prompt_border_color))
            .style(Style::default().bg(theme::BG));
        let prompt_inner = prompt_block.inner(chunks[5]);
        let prompt_para = Paragraph::new(self.prompt_input.value())
            .style(Style::default().fg(theme::TEXT))
            .wrap(Wrap { trim: false })
            .block(prompt_block);
        frame.render_widget(prompt_para, chunks[5]);

        // Recent directories dropdown (rendered over other content)
        if self.show_recent_dirs && !self.recent_dirs.is_empty() {
            let dropdown_height = self.recent_dirs.len() as u16 + 2; // +2 for borders
            let dropdown_area = ratatui::layout::Rect {
                x: chunks[3].x,
                y: chunks[3].y + chunks[3].height,
                width: chunks[3].width,
                height: dropdown_height.min(7), // max 5 items + 2 borders
            };
            frame.render_widget(Clear, dropdown_area);
            let items: Vec<ratatui::widgets::ListItem> = self
                .recent_dirs
                .iter()
                .enumerate()
                .map(|(i, d)| {
                    let style = if Some(i) == self.recent_dir_selected {
                        Style::default().fg(theme::TEXT).bg(theme::GRAY)
                    } else {
                        Style::default().fg(theme::GRAY_DIM)
                    };
                    ratatui::widgets::ListItem::new(Line::from(Span::styled(d.as_str(), style)))
                })
                .collect();
            let dropdown = ratatui::widgets::List::new(items).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme::ORANGE))
                    .title(" Recent Directories ")
                    .title_style(Style::default().fg(theme::ORANGE_BRIGHT))
                    .style(Style::default().bg(theme::BG)),
            );
            frame.render_widget(dropdown, dropdown_area);
        }

        // Hints + error
        let hint_text = if self.show_recent_dirs {
            "↑/↓: select  |  Enter: confirm  |  Esc: cancel"
        } else if self.focused_field == InputField::Directory && self.dir_suggestion.is_some() {
            if !self.recent_dirs.is_empty() {
                "→: complete  |  Ctrl+D: recent dirs  |  Tab: next  |  Enter: submit  |  Esc: quit"
            } else {
                "→: complete path  |  Tab: next field  |  Enter: submit  |  Esc: quit"
            }
        } else if self.focused_field == InputField::Directory && !self.recent_dirs.is_empty() {
            "Ctrl+D: recent dirs  |  Tab: next field  |  Enter: submit  |  Esc: quit"
        } else {
            "Tab: next field  |  Enter: submit  |  Esc: quit"
        };
        let hints = Paragraph::new(Line::from(Span::styled(
            hint_text,
            Style::default().fg(theme::GRAY_DIM),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(hints, chunks[6]);

        if let Some(ref err) = self.error_message {
            let error_para = Paragraph::new(Line::from(Span::styled(
                format!(" {err} "),
                Style::default().fg(Color::White).bg(theme::ERROR),
            )))
            .alignment(Alignment::Center);
            frame.render_widget(error_para, error_area);
        }

        // Place cursor in focused input
        let (cursor_input, cursor_area) = match self.focused_field {
            InputField::Title => (&self.title_input, title_inner),
            InputField::Directory => (&self.dir_input, dir_inner),
            InputField::Prompt => (&self.prompt_input, prompt_inner),
        };
        let visual_cursor = cursor_input.visual_cursor() as u16;
        let inner_width = cursor_area.width;
        let (cursor_x, cursor_y) = if self.focused_field == InputField::Prompt && inner_width > 0 {
            // Account for text wrapping in the prompt field
            let line = visual_cursor / inner_width;
            let col = visual_cursor % inner_width;
            (cursor_area.x + col, cursor_area.y + line)
        } else {
            (cursor_area.x + visual_cursor, cursor_area.y)
        };
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

    #[test]
    fn test_with_recent_dirs_defaults_to_most_recent() {
        let dirs = vec!["/home/user/project".to_string(), "/tmp".to_string()];
        let app = App::with_recent_dirs(dirs);
        assert_eq!(app.dir_input.value(), "/home/user/project");
        assert_eq!(app.recent_dirs.len(), 2);
    }

    #[test]
    fn test_with_empty_recent_dirs_defaults_to_cwd() {
        let app = App::with_recent_dirs(Vec::new());
        let cwd = std::env::current_dir()
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert_eq!(app.dir_input.value(), cwd);
    }

    fn ctrl_d() -> KeyEvent {
        KeyEvent {
            code: KeyCode::Char('d'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn test_ctrl_d_opens_recent_dirs_dropdown() {
        let dirs = vec!["/tmp".to_string(), "/home".to_string()];
        let mut app = App::with_recent_dirs(dirs);
        app.focused_field = InputField::Directory;
        app.handle_key(ctrl_d());
        assert!(app.show_recent_dirs);
        assert_eq!(app.recent_dir_selected, Some(0));
    }

    #[test]
    fn test_ctrl_d_noop_without_recent_dirs() {
        let mut app = App::new();
        app.focused_field = InputField::Directory;
        app.handle_key(ctrl_d());
        assert!(!app.show_recent_dirs);
    }

    #[test]
    fn test_ctrl_d_noop_on_other_fields() {
        let dirs = vec!["/tmp".to_string()];
        let mut app = App::with_recent_dirs(dirs);
        app.focused_field = InputField::Title;
        app.handle_key(ctrl_d());
        assert!(!app.show_recent_dirs);
    }

    #[test]
    fn test_recent_dirs_navigate_down_and_up() {
        let dirs = vec!["/a".to_string(), "/b".to_string(), "/c".to_string()];
        let mut app = App::with_recent_dirs(dirs);
        app.focused_field = InputField::Directory;
        app.handle_key(ctrl_d());

        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.recent_dir_selected, Some(1));

        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.recent_dir_selected, Some(2));

        // Wraps around
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.recent_dir_selected, Some(0));

        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.recent_dir_selected, Some(2));
    }

    #[test]
    fn test_recent_dirs_enter_selects() {
        let dirs = vec!["/tmp".to_string(), "/home".to_string()];
        let mut app = App::with_recent_dirs(dirs);
        app.focused_field = InputField::Directory;
        app.handle_key(ctrl_d());
        app.handle_key(key(KeyCode::Down)); // select /home
        app.handle_key(key(KeyCode::Enter));

        assert!(!app.show_recent_dirs);
        assert_eq!(app.dir_input.value(), "/home");
    }

    #[test]
    fn test_recent_dirs_esc_cancels() {
        let dirs = vec!["/tmp".to_string()];
        let mut app = App::with_recent_dirs(dirs);
        app.focused_field = InputField::Directory;
        let original_dir = app.dir_input.value().to_string();
        app.handle_key(ctrl_d());
        app.handle_key(key(KeyCode::Esc));

        assert!(!app.show_recent_dirs);
        assert_eq!(app.dir_input.value(), original_dir);
    }
}
