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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputField {
    Title,
    Directory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    None,
    Quit,
    Submit { title: String, directory: String },
}

#[derive(Debug)]
pub struct App {
    pub running: bool,
    pub focused_field: InputField,
    pub title_input: Input,
    pub dir_input: Input,
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
            KeyCode::Tab | KeyCode::BackTab => {
                self.toggle_focus();
                Action::None
            }
            KeyCode::Up | KeyCode::Down => {
                self.toggle_focus();
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
                    }
                }
                self.error_message = None;
                Action::None
            }
        }
    }

    fn toggle_focus(&mut self) {
        self.focused_field = match self.focused_field {
            InputField::Title => InputField::Directory,
            InputField::Directory => InputField::Title,
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

        Action::Submit { title, directory }
    }

    pub fn draw(&self, frame: &mut Frame) {
        let area = frame.area();

        // Centered form: 60 wide, 12 tall
        let form_width = 60u16.min(area.width.saturating_sub(2));
        let form_height = 12u16.min(area.height.saturating_sub(2));

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

        // Layout inside form: label+input for title, label+input for directory, hint, error
        let chunks = Layout::vertical([
            Constraint::Length(1), // Title label
            Constraint::Length(3), // Title input
            Constraint::Length(1), // Directory label
            Constraint::Length(3), // Directory input
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
        let dir_para = Paragraph::new(self.dir_input.value()).block(dir_block);
        frame.render_widget(dir_para, chunks[3]);

        // Hints + error
        let mut hint_lines: Vec<Line> = vec![Line::from(
            Span::raw("Tab: switch  |  Enter: submit  |  Esc: quit").dark_gray(),
        )];

        if let Some(ref err) = self.error_message {
            hint_lines.push(Line::from(Span::raw(err.as_str()).fg(Color::Red)));
        }

        let hints = Paragraph::new(hint_lines);
        frame.render_widget(hints, chunks[4]);

        // Place cursor in focused input
        let (cursor_input, cursor_area) = match self.focused_field {
            InputField::Title => (&self.title_input, title_inner),
            InputField::Directory => (&self.dir_input, dir_inner),
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
    fn test_tab_toggles_focus() {
        let mut app = App::new();
        assert_eq!(app.focused_field, InputField::Title);

        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.focused_field, InputField::Directory);

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
    fn test_up_down_toggles_focus() {
        let mut app = App::new();
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.focused_field, InputField::Directory);
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.focused_field, InputField::Title);
    }
}
