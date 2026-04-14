use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Flex, Layout, Position},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use std::path::Path;
use tui_input::Input;
use tui_input::backend::crossterm::EventHandler;

use crate::theme;

/// Break text into lines of at most `width` characters (character-level wrapping).
/// Returns a Vec of line strings. This matches the cursor math which uses
/// `visual_cursor % width` and `visual_cursor / width`.
fn char_wrap_lines(text: &str, width: usize) -> Vec<String> {
    if width == 0 || text.is_empty() {
        return vec![text.to_string()];
    }
    let chars: Vec<char> = text.chars().collect();
    chars.chunks(width).map(|c| c.iter().collect()).collect()
}

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
pub enum FormMode {
    /// Full form: title, directory, git mode, prompt, claude args
    NewTask,
    /// Simplified form: title + directory only (plain tmux session)
    NewTmuxSession,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitMode {
    Worktree,
    Branch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputField {
    Title,
    Directory,
    GitMode,
    BranchName,
    Prompt,
    ClaudeArgs,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    None,
    Quit,
    Submit {
        title: String,
        directory: String,
        git_mode: GitMode,
        branch_name: Option<String>,
        prompt: Option<String>,
        claude_args: Option<String>,
    },
    SubmitTmuxSession {
        title: String,
        directory: String,
    },
}

#[derive(Debug)]
pub struct App {
    pub running: bool,
    pub form_mode: FormMode,
    pub focused_field: InputField,
    pub title_input: Input,
    pub dir_input: Input,
    pub git_mode: GitMode,
    pub branch_name_input: Input,
    pub prompt_input: Input,
    pub claude_args_input: Input,
    pub dir_suggestion: Option<String>,
    pub error_message: Option<String>,
    pub recent_dirs: Vec<String>,
    pub recent_dir_selected: Option<usize>,
    pub recent_dir_scroll: usize,
    pub show_recent_dirs: bool,
    pub recent_dir_query: String,
    pub show_advanced: bool,
}

impl App {
    #[cfg(test)]
    pub fn new() -> Self {
        Self::with_recent_dirs(Vec::new())
    }

    pub fn with_recent_dirs(recent_dirs: Vec<String>) -> Self {
        Self::with_recent_dirs_and_mode(recent_dirs, FormMode::NewTask)
    }

    pub fn with_recent_dirs_and_mode(recent_dirs: Vec<String>, form_mode: FormMode) -> Self {
        let default_dir = recent_dirs.first().cloned().unwrap_or_else(|| {
            std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default()
        });

        Self {
            running: true,
            form_mode,
            focused_field: InputField::Title,
            title_input: Input::default(),
            dir_input: Input::new(default_dir),
            git_mode: GitMode::Worktree,
            branch_name_input: Input::default(),
            prompt_input: Input::default(),
            claude_args_input: Input::default(),
            dir_suggestion: None,
            error_message: None,
            recent_dirs,
            recent_dir_selected: None,
            recent_dir_scroll: 0,
            show_recent_dirs: false,
            recent_dir_query: String::new(),
            show_advanced: false,
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

        // Ctrl+A toggles advanced settings (not available in tmux session mode)
        if self.form_mode == FormMode::NewTask
            && key.code == KeyCode::Char('a')
            && key
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL)
        {
            self.show_advanced = !self.show_advanced;
            // If hiding advanced and focused on an advanced field, move to Prompt
            if !self.show_advanced
                && matches!(
                    self.focused_field,
                    InputField::GitMode | InputField::BranchName | InputField::ClaudeArgs
                )
            {
                self.focused_field = InputField::Prompt;
            }
            return Action::None;
        }

        match key.code {
            KeyCode::Esc => {
                self.quit();
                Action::Quit
            }
            KeyCode::Tab
                if self.focused_field == InputField::Directory
                    && self.dir_suggestion.is_some()
                    && self.cursor_at_end() =>
            {
                if !self.complete_directory() {
                    self.next_field();
                }
                Action::None
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
            KeyCode::Left | KeyCode::Right if self.focused_field == InputField::GitMode => {
                self.git_mode = match self.git_mode {
                    GitMode::Worktree => GitMode::Branch,
                    GitMode::Branch => GitMode::Worktree,
                };
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
                    self.recent_dir_scroll = 0;
                    self.recent_dir_query.clear();
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
                    InputField::GitMode => {
                        // Only Left/Right toggle (handled above); ignore other chars
                    }
                    InputField::BranchName => {
                        self.branch_name_input
                            .handle_event(&crossterm::event::Event::Key(key));
                    }
                    InputField::Prompt => {
                        self.prompt_input
                            .handle_event(&crossterm::event::Event::Key(key));
                    }
                    InputField::ClaudeArgs => {
                        self.claude_args_input
                            .handle_event(&crossterm::event::Event::Key(key));
                    }
                }
                self.error_message = None;
                Action::None
            }
        }
    }

    /// Maximum number of items visible in the recent-dirs dropdown.
    const RECENT_DIRS_VISIBLE: usize = 10;

    fn filtered_recent_dirs(&self) -> Vec<&str> {
        if self.recent_dir_query.is_empty() {
            self.recent_dirs.iter().map(|s| s.as_str()).collect()
        } else {
            let q = self.recent_dir_query.to_lowercase();
            self.recent_dirs
                .iter()
                .filter(|d| d.to_lowercase().contains(&q))
                .map(|s| s.as_str())
                .collect()
        }
    }

    fn adjust_recent_dir_scroll(&mut self) {
        if let Some(sel) = self.recent_dir_selected {
            let visible = Self::RECENT_DIRS_VISIBLE;
            if sel < self.recent_dir_scroll {
                self.recent_dir_scroll = sel;
            } else if sel >= self.recent_dir_scroll + visible {
                self.recent_dir_scroll = sel + 1 - visible;
            }
        } else {
            self.recent_dir_scroll = 0;
        }
    }

    fn handle_recent_dirs_key(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Esc => {
                self.show_recent_dirs = false;
                self.recent_dir_selected = None;
                self.recent_dir_scroll = 0;
                self.recent_dir_query.clear();
                Action::None
            }
            KeyCode::Up | KeyCode::BackTab => {
                let n = self.filtered_recent_dirs().len();
                if n > 0 {
                    self.recent_dir_selected = Some(match self.recent_dir_selected {
                        Some(i) if i > 0 => i - 1,
                        _ => n - 1,
                    });
                }
                self.adjust_recent_dir_scroll();
                Action::None
            }
            KeyCode::Down | KeyCode::Tab => {
                let n = self.filtered_recent_dirs().len();
                if n > 0 {
                    self.recent_dir_selected = Some(match self.recent_dir_selected {
                        Some(i) if i < n - 1 => i + 1,
                        _ => 0,
                    });
                }
                self.adjust_recent_dir_scroll();
                Action::None
            }
            KeyCode::Enter => {
                let selected = self
                    .recent_dir_selected
                    .and_then(|i| self.filtered_recent_dirs().get(i).map(|s| s.to_string()));
                if let Some(dir) = selected {
                    self.dir_input = Input::new(dir.clone());
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
                self.recent_dir_scroll = 0;
                self.recent_dir_query.clear();
                Action::None
            }
            KeyCode::Backspace => {
                self.recent_dir_query.pop();
                let n = self.filtered_recent_dirs().len();
                self.recent_dir_selected = if n > 0 { Some(0) } else { None };
                self.recent_dir_scroll = 0;
                Action::None
            }
            KeyCode::Char(c) if !key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                self.recent_dir_query.push(c);
                let n = self.filtered_recent_dirs().len();
                self.recent_dir_selected = if n > 0 { Some(0) } else { None };
                self.recent_dir_scroll = 0;
                Action::None
            }
            _ => Action::None,
        }
    }

    fn cursor_at_end(&self) -> bool {
        self.dir_input.visual_cursor() >= self.dir_input.value().len()
    }

    /// Attempt to complete the directory input. Returns true if progress was made
    /// (input changed), false if no progress (e.g. already at a fully-expanded path).
    fn complete_directory(&mut self) -> bool {
        let current = self.dir_input.value().to_string();
        if let Some((completed, _suggestion)) = complete_path(&current) {
            if completed == current {
                return false;
            }
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
            true
        } else {
            false
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
        if self.form_mode == FormMode::NewTmuxSession {
            self.focused_field = match self.focused_field {
                InputField::Title => InputField::Directory,
                _ => InputField::Title,
            };
            return;
        }
        self.focused_field = match self.focused_field {
            InputField::Title => InputField::Directory,
            InputField::Directory => {
                if self.show_advanced {
                    InputField::GitMode
                } else {
                    InputField::Prompt
                }
            }
            InputField::GitMode => {
                if self.git_mode == GitMode::Branch {
                    InputField::BranchName
                } else {
                    InputField::Prompt
                }
            }
            InputField::BranchName => InputField::Prompt,
            InputField::Prompt => {
                if self.show_advanced {
                    InputField::ClaudeArgs
                } else {
                    InputField::Title
                }
            }
            InputField::ClaudeArgs => InputField::Title,
        };
    }

    fn prev_field(&mut self) {
        if self.form_mode == FormMode::NewTmuxSession {
            self.focused_field = match self.focused_field {
                InputField::Directory => InputField::Title,
                _ => InputField::Directory,
            };
            return;
        }
        self.focused_field = match self.focused_field {
            InputField::Title => {
                if self.show_advanced {
                    InputField::ClaudeArgs
                } else {
                    InputField::Prompt
                }
            }
            InputField::Directory => InputField::Title,
            InputField::GitMode => InputField::Directory,
            InputField::BranchName => InputField::GitMode,
            InputField::Prompt => {
                if self.show_advanced {
                    if self.git_mode == GitMode::Branch {
                        InputField::BranchName
                    } else {
                        InputField::GitMode
                    }
                } else {
                    InputField::Directory
                }
            }
            InputField::ClaudeArgs => InputField::Prompt,
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

        if self.form_mode == FormMode::NewTmuxSession {
            return Action::SubmitTmuxSession { title, directory };
        }

        let branch_name = if self.git_mode == GitMode::Branch {
            let name = self.branch_name_input.value().trim().to_string();
            if name.is_empty() {
                self.error_message = Some("Branch name cannot be empty".to_string());
                self.focused_field = InputField::BranchName;
                return Action::None;
            }
            Some(name)
        } else {
            None
        };

        let prompt_raw = self.prompt_input.value().trim().to_string();
        let prompt = if prompt_raw.is_empty() {
            None
        } else {
            Some(prompt_raw)
        };

        let args_raw = self.claude_args_input.value().trim().to_string();
        let claude_args = if args_raw.is_empty() {
            None
        } else {
            Some(args_raw)
        };

        Action::Submit {
            title,
            directory,
            git_mode: self.git_mode,
            branch_name,
            prompt,
            claude_args,
        }
    }

    pub fn draw(&self, frame: &mut Frame) {
        if self.form_mode == FormMode::NewTmuxSession {
            self.draw_tmux_session_form(frame);
            return;
        }

        let area = frame.area();

        // Centered form: 90 wide, dynamically sized vertically
        let form_width = 90u16.min(area.width.saturating_sub(2));

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
        let max_prompt_height = area.height.saturating_sub(22); // leave room for other fields
        let prompt_box_height = (prompt_lines + 2).min(max_prompt_height).max(3);

        // Extra height for advanced fields
        let advanced_extra = if self.show_advanced {
            let branch_extra = if self.git_mode == GitMode::Branch {
                4
            } else {
                0
            };
            2 + branch_extra + 4 // git mode (label + selector) + branch (conditional) + args (label + input)
        } else {
            0
        };

        // Base: 2 (outer border) + 1 (title label) + 3 (title input) + 1 (dir label) + 3 (dir input)
        //       + 1 (prompt label) + prompt_box_height + 1 (hints) + 1 (tab bar) = 11 + prompt_box_height + 1
        let form_height =
            (13 + advanced_extra as u16 + prompt_box_height).min(area.height.saturating_sub(2));
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
        let outer_chunks =
            Layout::vertical([Constraint::Length(form_height), Constraint::Length(1)])
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

        // Build layout constraints dynamically based on mode
        let mut constraints = vec![
            Constraint::Length(1), // Tab bar
            Constraint::Length(1), // Title label
            Constraint::Length(3), // Title input
            Constraint::Length(1), // Directory label
            Constraint::Length(3), // Directory input
        ];
        if self.show_advanced {
            constraints.push(Constraint::Length(1)); // Git mode label
            constraints.push(Constraint::Length(1)); // Git mode selector
            if self.git_mode == GitMode::Branch {
                constraints.push(Constraint::Length(1)); // Branch name label
                constraints.push(Constraint::Length(3)); // Branch name input
            }
        }
        constraints.extend_from_slice(&[
            Constraint::Length(1),                 // Prompt label
            Constraint::Length(prompt_box_height), // Prompt input
        ]);
        if self.show_advanced {
            constraints.push(Constraint::Length(1)); // Claude args label
            constraints.push(Constraint::Length(3)); // Claude args input
        }
        constraints.push(Constraint::Min(1)); // Hints
        let chunks = Layout::vertical(constraints).split(inner);

        // Track chunk indices dynamically
        let tab_bar_idx = 0;
        let title_label_idx = 1;
        let title_input_idx = 2;
        let dir_label_idx = 3;
        let dir_input_idx = 4;
        let mut next_idx = 5;

        let git_mode_label_idx;
        let git_mode_selector_idx;
        let branch_label_idx;
        let branch_input_idx;
        if self.show_advanced {
            git_mode_label_idx = Some(next_idx);
            next_idx += 1;
            git_mode_selector_idx = Some(next_idx);
            next_idx += 1;
            if self.git_mode == GitMode::Branch {
                branch_label_idx = Some(next_idx);
                next_idx += 1;
                branch_input_idx = Some(next_idx);
                next_idx += 1;
            } else {
                branch_label_idx = None;
                branch_input_idx = None;
            }
        } else {
            git_mode_label_idx = None;
            git_mode_selector_idx = None;
            branch_label_idx = None;
            branch_input_idx = None;
        }

        let prompt_label_idx = next_idx;
        next_idx += 1;
        let prompt_input_idx = next_idx;
        next_idx += 1;

        let args_label_idx;
        let args_input_idx;
        if self.show_advanced {
            args_label_idx = Some(next_idx);
            next_idx += 1;
            args_input_idx = Some(next_idx);
            next_idx += 1;
        } else {
            args_label_idx = None;
            args_input_idx = None;
        }
        let hints_idx = next_idx;

        // Tab bar
        let (basic_style, advanced_style) = if self.show_advanced {
            (
                Style::default().fg(theme::GRAY_DIM).bg(theme::BG),
                Style::default().fg(theme::BG).bg(theme::ORANGE_BRIGHT),
            )
        } else {
            (
                Style::default().fg(theme::BG).bg(theme::ORANGE_BRIGHT),
                Style::default().fg(theme::GRAY_DIM).bg(theme::BG),
            )
        };
        let tab_line = Line::from(vec![
            Span::styled(" Basic ", basic_style),
            Span::styled("  ", Style::default().bg(theme::BG)),
            Span::styled(" Advanced ", advanced_style),
            Span::styled(
                "  (Ctrl+A to toggle)",
                Style::default().fg(theme::GRAY_DIM).bg(theme::BG),
            ),
        ]);
        let tab_para = Paragraph::new(tab_line).style(Style::default().bg(theme::BG));
        frame.render_widget(tab_para, chunks[tab_bar_idx]);

        // Title label
        let title_label =
            Paragraph::new("Title:").style(Style::default().fg(theme::TEXT).bg(theme::BG));
        frame.render_widget(title_label, chunks[title_label_idx]);

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
        let title_inner = title_block.inner(chunks[title_input_idx]);
        let title_para = Paragraph::new(self.title_input.value())
            .style(Style::default().fg(theme::TEXT))
            .block(title_block);
        frame.render_widget(title_para, chunks[title_input_idx]);

        // Directory label
        let dir_label =
            Paragraph::new("Directory:").style(Style::default().fg(theme::TEXT).bg(theme::BG));
        frame.render_widget(dir_label, chunks[dir_label_idx]);

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
        let dir_inner = dir_block.inner(chunks[dir_input_idx]);
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
        frame.render_widget(dir_para, chunks[dir_input_idx]);

        // Git mode + Branch name (only when advanced is shown)
        let mut branch_inner = None;
        if self.show_advanced {
            if let (Some(gml_idx), Some(gms_idx)) = (git_mode_label_idx, git_mode_selector_idx) {
                // Git mode label
                let git_mode_label = Paragraph::new("Git mode:")
                    .style(Style::default().fg(theme::TEXT).bg(theme::BG));
                frame.render_widget(git_mode_label, chunks[gml_idx]);

                // Git mode selector — inline toggle
                let git_mode_focused = self.focused_field == InputField::GitMode;
                let (worktree_style, branch_style) = match self.git_mode {
                    GitMode::Worktree => (
                        Style::default().fg(theme::BG).bg(theme::ORANGE_BRIGHT),
                        Style::default().fg(theme::GRAY_DIM).bg(theme::BG),
                    ),
                    GitMode::Branch => (
                        Style::default().fg(theme::GRAY_DIM).bg(theme::BG),
                        Style::default().fg(theme::BG).bg(theme::ORANGE_BRIGHT),
                    ),
                };
                let selector_line = Line::from(vec![
                    Span::styled(
                        if git_mode_focused { "< " } else { "  " },
                        Style::default().fg(theme::GRAY_DIM),
                    ),
                    Span::styled(" Worktree ", worktree_style),
                    Span::styled("  ", Style::default().fg(theme::TEXT).bg(theme::BG)),
                    Span::styled(" Branch ", branch_style),
                    Span::styled(
                        if git_mode_focused { " >" } else { "  " },
                        Style::default().fg(theme::GRAY_DIM),
                    ),
                ]);
                let selector_para =
                    Paragraph::new(selector_line).style(Style::default().bg(theme::BG));
                frame.render_widget(selector_para, chunks[gms_idx]);
            }

            // Branch name (only when Branch mode)
            if self.git_mode == GitMode::Branch
                && let (Some(bl_idx), Some(bi_idx)) = (branch_label_idx, branch_input_idx)
            {
                let branch_label = Paragraph::new("Branch name:")
                    .style(Style::default().fg(theme::TEXT).bg(theme::BG));
                frame.render_widget(branch_label, chunks[bl_idx]);

                let branch_border_color = if self.focused_field == InputField::BranchName {
                    theme::ORANGE_BRIGHT
                } else {
                    theme::GRAY
                };
                let branch_block = Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(branch_border_color))
                    .style(Style::default().bg(theme::BG));
                let bi = branch_block.inner(chunks[bi_idx]);
                branch_inner = Some(bi);
                let branch_para = Paragraph::new(self.branch_name_input.value())
                    .style(Style::default().fg(theme::TEXT))
                    .block(branch_block);
                frame.render_widget(branch_para, chunks[bi_idx]);
            }
        }

        // Prompt label
        let prompt_label = Paragraph::new("Prompt (optional):")
            .style(Style::default().fg(theme::TEXT).bg(theme::BG));
        frame.render_widget(prompt_label, chunks[prompt_label_idx]);

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
        let prompt_inner = prompt_block.inner(chunks[prompt_input_idx]);
        let wrapped_lines = char_wrap_lines(self.prompt_input.value(), prompt_inner.width as usize);
        let lines: Vec<Line> = wrapped_lines.into_iter().map(Line::from).collect();
        let prompt_para = Paragraph::new(lines)
            .style(Style::default().fg(theme::TEXT))
            .block(prompt_block);
        frame.render_widget(prompt_para, chunks[prompt_input_idx]);

        // Claude args (only when advanced is shown)
        let mut args_inner = None;
        if self.show_advanced
            && let (Some(al_idx), Some(ai_idx)) = (args_label_idx, args_input_idx)
        {
            let args_label = Paragraph::new("Additional CLI args (optional):")
                .style(Style::default().fg(theme::TEXT).bg(theme::BG));
            frame.render_widget(args_label, chunks[al_idx]);

            let args_border_color = if self.focused_field == InputField::ClaudeArgs {
                theme::ORANGE_BRIGHT
            } else {
                theme::GRAY
            };
            let args_block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(args_border_color))
                .style(Style::default().bg(theme::BG));
            args_inner = Some(args_block.inner(chunks[ai_idx]));
            let args_para = Paragraph::new(self.claude_args_input.value())
                .style(Style::default().fg(theme::TEXT))
                .block(args_block);
            frame.render_widget(args_para, chunks[ai_idx]);
        }

        // Hints + error
        let hint_text = if self.show_recent_dirs {
            "type to filter  |  ↑/↓: select  |  Enter: confirm  |  Esc: cancel"
        } else if self.focused_field == InputField::GitMode {
            "←/→: toggle  |  Tab: next  |  Ctrl+A: basic  |  Enter: submit  |  Esc: quit"
        } else if self.focused_field == InputField::Directory && self.dir_suggestion.is_some() {
            if !self.recent_dirs.is_empty() {
                "Tab/→: complete  |  Ctrl+D: recent dirs  |  Ctrl+A: advanced  |  Enter: submit  |  Esc: quit"
            } else {
                "Tab/→: complete  |  Ctrl+A: advanced  |  Enter: submit  |  Esc: quit"
            }
        } else if self.focused_field == InputField::Directory && !self.recent_dirs.is_empty() {
            "Ctrl+D: recent dirs  |  Ctrl+A: advanced  |  Enter: submit  |  Esc: quit"
        } else if self.show_advanced {
            "Tab: next  |  Ctrl+A: basic  |  Enter: submit  |  Esc: quit"
        } else {
            "Tab: next  |  Ctrl+A: advanced  |  Enter: submit  |  Esc: quit"
        };
        let hints = Paragraph::new(Line::from(Span::styled(
            hint_text,
            Style::default().fg(theme::GRAY_DIM),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(hints, chunks[hints_idx]);

        if let Some(ref err) = self.error_message {
            let error_para = Paragraph::new(Line::from(Span::styled(
                format!(" {err} "),
                Style::default().fg(Color::White).bg(theme::ERROR),
            )))
            .alignment(Alignment::Center);
            frame.render_widget(error_para, error_area);
        }

        // Recent directories dropdown (rendered last so it overlays hints/error)
        if self.show_recent_dirs && !self.recent_dirs.is_empty() {
            let filtered = self.filtered_recent_dirs();
            let visible = filtered.len().min(Self::RECENT_DIRS_VISIBLE);
            let dropdown_height = visible as u16 + 2; // +2 for borders
            let dropdown_area = ratatui::layout::Rect {
                x: chunks[dir_input_idx].x,
                y: chunks[dir_input_idx].y + chunks[dir_input_idx].height,
                width: chunks[dir_input_idx].width,
                height: dropdown_height,
            };
            frame.render_widget(Clear, dropdown_area);
            let items: Vec<ratatui::widgets::ListItem> = filtered
                .iter()
                .enumerate()
                .skip(self.recent_dir_scroll)
                .take(visible)
                .map(|(i, d)| {
                    let style = if Some(i) == self.recent_dir_selected {
                        Style::default().fg(theme::TEXT).bg(theme::GRAY)
                    } else {
                        Style::default().fg(theme::GRAY_DIM)
                    };
                    ratatui::widgets::ListItem::new(Line::from(Span::styled(*d, style)))
                })
                .collect();
            let title = if self.recent_dir_query.is_empty() {
                format!(
                    " Recent Directories ({}/{}) ",
                    self.recent_dir_selected.map_or(0, |i| i + 1),
                    filtered.len()
                )
            } else {
                format!(
                    " > {}  ({}/{}) ",
                    self.recent_dir_query,
                    self.recent_dir_selected.map_or(0, |i| i + 1),
                    filtered.len()
                )
            };
            let dropdown = ratatui::widgets::List::new(items).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme::ORANGE))
                    .title(title)
                    .title_style(Style::default().fg(theme::ORANGE_BRIGHT))
                    .style(Style::default().bg(theme::BG)),
            );
            frame.render_widget(dropdown, dropdown_area);
        }

        // Place cursor in focused input
        match self.focused_field {
            InputField::GitMode => {
                // No cursor for the selector
            }
            _ => {
                let (cursor_input, cursor_area) = match self.focused_field {
                    InputField::Title => (&self.title_input, title_inner),
                    InputField::Directory => (&self.dir_input, dir_inner),
                    InputField::BranchName => {
                        (&self.branch_name_input, branch_inner.unwrap_or(title_inner))
                    }
                    InputField::Prompt => (&self.prompt_input, prompt_inner),
                    InputField::ClaudeArgs => {
                        (&self.claude_args_input, args_inner.unwrap_or(title_inner))
                    }
                    InputField::GitMode => unreachable!(),
                };
                let visual_cursor = cursor_input.visual_cursor() as u16;
                let inner_width = cursor_area.width;
                let (cursor_x, cursor_y) =
                    if self.focused_field == InputField::Prompt && inner_width > 0 {
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
    }

    fn draw_tmux_session_form(&self, frame: &mut Frame) {
        let area = frame.area();
        let form_width = 90u16.min(area.width.saturating_sub(2));
        // 2 (outer border) + 1 (title label) + 3 (title input) + 1 (dir label) + 3 (dir input) + 1 (hints) = 11
        let form_height = 11u16.min(area.height.saturating_sub(2));
        let total_height = form_height + 1; // +1 for error line

        let vertical = Layout::vertical([Constraint::Length(total_height)])
            .flex(Flex::Center)
            .split(area);
        let horizontal = Layout::horizontal([Constraint::Length(form_width)])
            .flex(Flex::Center)
            .split(vertical[0]);
        let outer_area = horizontal[0];

        let outer_chunks =
            Layout::vertical([Constraint::Length(form_height), Constraint::Length(1)])
                .split(outer_area);
        let form_area = outer_chunks[0];
        let error_area = outer_chunks[1];

        frame.render_widget(Clear, form_area);
        frame.render_widget(
            Block::default().style(Style::default().bg(theme::BG)),
            form_area,
        );

        let outer_block = Block::default()
            .title(" New Tmux Session ")
            .title_style(Style::default().fg(theme::ORANGE_BRIGHT))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::ORANGE))
            .style(Style::default().bg(theme::BG));
        let inner = outer_block.inner(form_area);
        frame.render_widget(outer_block, form_area);

        let chunks = Layout::vertical([
            Constraint::Length(1), // Title label
            Constraint::Length(3), // Title input
            Constraint::Length(1), // Directory label
            Constraint::Length(3), // Directory input
            Constraint::Min(1),    // Hints
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

        // Hints
        let hint_text = if self.show_recent_dirs {
            "type to filter  |  ↑/↓: select  |  Enter: confirm  |  Esc: cancel"
        } else if self.focused_field == InputField::Directory && self.dir_suggestion.is_some() {
            if !self.recent_dirs.is_empty() {
                "Tab/→: complete  |  Ctrl+D: recent dirs  |  Enter: submit  |  Esc: back"
            } else {
                "Tab/→: complete  |  Enter: submit  |  Esc: back"
            }
        } else if self.focused_field == InputField::Directory && !self.recent_dirs.is_empty() {
            "Ctrl+D: recent dirs  |  Enter: submit  |  Esc: back"
        } else {
            "Tab: next  |  Enter: submit  |  Esc: back"
        };
        let hints = Paragraph::new(Line::from(Span::styled(
            hint_text,
            Style::default().fg(theme::GRAY_DIM),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(hints, chunks[4]);

        // Error message
        if let Some(ref err) = self.error_message {
            let error_para = Paragraph::new(Line::from(Span::styled(
                format!(" {err} "),
                Style::default().fg(Color::White).bg(theme::ERROR),
            )))
            .alignment(Alignment::Center);
            frame.render_widget(error_para, error_area);
        }

        // Recent directories dropdown
        if self.show_recent_dirs && !self.recent_dirs.is_empty() {
            let filtered = self.filtered_recent_dirs();
            let visible = filtered.len().min(Self::RECENT_DIRS_VISIBLE);
            let dropdown_height = visible as u16 + 2;
            let dropdown_area = ratatui::layout::Rect {
                x: chunks[3].x,
                y: chunks[3].y + chunks[3].height,
                width: chunks[3].width,
                height: dropdown_height,
            };
            frame.render_widget(Clear, dropdown_area);
            let items: Vec<ratatui::widgets::ListItem> = filtered
                .iter()
                .enumerate()
                .skip(self.recent_dir_scroll)
                .take(visible)
                .map(|(i, d)| {
                    let style = if Some(i) == self.recent_dir_selected {
                        Style::default().fg(theme::TEXT).bg(theme::GRAY)
                    } else {
                        Style::default().fg(theme::GRAY_DIM)
                    };
                    ratatui::widgets::ListItem::new(Line::from(Span::styled(*d, style)))
                })
                .collect();
            let title = if self.recent_dir_query.is_empty() {
                format!(
                    " Recent Directories ({}/{}) ",
                    self.recent_dir_selected.map_or(0, |i| i + 1),
                    filtered.len()
                )
            } else {
                format!(
                    " > {}  ({}/{}) ",
                    self.recent_dir_query,
                    self.recent_dir_selected.map_or(0, |i| i + 1),
                    filtered.len()
                )
            };
            let dropdown = ratatui::widgets::List::new(items).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme::ORANGE))
                    .title(title)
                    .title_style(Style::default().fg(theme::ORANGE_BRIGHT))
                    .style(Style::default().bg(theme::BG)),
            );
            frame.render_widget(dropdown, dropdown_area);
        }

        // Place cursor
        let (cursor_input, cursor_area) = match self.focused_field {
            InputField::Title => (&self.title_input, title_inner),
            InputField::Directory => (&self.dir_input, dir_inner),
            _ => (&self.title_input, title_inner),
        };
        let visual_cursor = cursor_input.visual_cursor() as u16;
        frame.set_cursor_position(Position::new(cursor_area.x + visual_cursor, cursor_area.y));
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
    fn test_tab_cycles_basic_fields() {
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
    fn test_tab_cycles_all_fields_advanced() {
        let mut app = App::new();
        app.show_advanced = true;
        assert_eq!(app.focused_field, InputField::Title);

        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.focused_field, InputField::Directory);

        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.focused_field, InputField::GitMode);

        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.focused_field, InputField::Prompt);

        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.focused_field, InputField::ClaudeArgs);

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
                git_mode: GitMode::Worktree,
                branch_name: None,
                prompt: None,
                claude_args: None,
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
    fn test_up_down_cycles_basic_focus() {
        let mut app = App::new();
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.focused_field, InputField::Directory);
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.focused_field, InputField::Prompt);
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.focused_field, InputField::Title);
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.focused_field, InputField::Prompt);
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.focused_field, InputField::Directory);
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.focused_field, InputField::Title);
    }

    #[test]
    fn test_up_down_cycles_advanced_focus() {
        let mut app = App::new();
        app.show_advanced = true;
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.focused_field, InputField::Directory);
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.focused_field, InputField::GitMode);
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.focused_field, InputField::Prompt);
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.focused_field, InputField::ClaudeArgs);
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.focused_field, InputField::Prompt);
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.focused_field, InputField::GitMode);
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
                git_mode: GitMode::Worktree,
                branch_name: None,
                prompt: Some("fix the bug".to_string()),
                claude_args: None,
            }
        );
    }

    #[test]
    fn test_submit_with_claude_args() {
        let mut app = App::new();
        app.show_advanced = true;
        for ch in "my task".chars() {
            app.handle_key(key(KeyCode::Char(ch)));
        }
        // Skip to claude args field
        app.handle_key(key(KeyCode::Tab)); // -> Directory
        app.handle_key(key(KeyCode::Tab)); // -> GitMode
        app.handle_key(key(KeyCode::Tab)); // -> Prompt
        app.handle_key(key(KeyCode::Tab)); // -> ClaudeArgs
        for ch in "--model sonnet".chars() {
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
                git_mode: GitMode::Worktree,
                branch_name: None,
                prompt: None,
                claude_args: Some("--model sonnet".to_string()),
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
    fn test_tab_no_progress_moves_to_next_field() {
        // When complete_directory makes no progress (completed == current input),
        // Tab should fall through to next_field instead of silently eating the keypress.
        let mut app = App::new();
        app.focused_field = InputField::Directory;
        // Use a nonexistent path so complete_path returns None → complete_directory
        // returns false → Tab must move field. We also set dir_suggestion manually
        // to trigger the Tab guard condition.
        app.dir_input = Input::new("/nonexistent_abc_xyz_123/".to_string());
        for _ in 0..30 {
            app.dir_input
                .handle_event(&crossterm::event::Event::Key(key(KeyCode::Right)));
        }
        app.dir_suggestion = Some("ghost".to_string());
        app.handle_key(key(KeyCode::Tab));
        // complete_path returns None → complete_directory returns false → next_field called
        assert_eq!(app.focused_field, InputField::Prompt);
    }

    #[test]
    fn test_tab_on_directory_moves_to_prompt_in_basic() {
        let mut app = App::new();
        app.focused_field = InputField::Directory;
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.focused_field, InputField::Prompt);
    }

    #[test]
    fn test_tab_on_directory_moves_to_git_mode_in_advanced() {
        let mut app = App::new();
        app.show_advanced = true;
        app.focused_field = InputField::Directory;
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.focused_field, InputField::GitMode);
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

    #[test]
    fn test_recent_dirs_scroll_adjusts_on_navigate_down() {
        // Create more dirs than RECENT_DIRS_VISIBLE (10)
        let dirs: Vec<String> = (0..15).map(|i| format!("/dir{i}")).collect();
        let mut app = App::with_recent_dirs(dirs);
        app.focused_field = InputField::Directory;
        app.handle_key(ctrl_d());
        assert_eq!(app.recent_dir_scroll, 0);

        // Navigate down past the visible window
        for _ in 0..10 {
            app.handle_key(key(KeyCode::Down));
        }
        assert_eq!(app.recent_dir_selected, Some(10));
        assert_eq!(app.recent_dir_scroll, 1); // scrolled to keep selection visible

        // Navigate back up — still in view
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.recent_dir_selected, Some(9));
        assert_eq!(app.recent_dir_scroll, 1);

        // Navigate up past scroll offset
        for _ in 0..9 {
            app.handle_key(key(KeyCode::Up));
        }
        assert_eq!(app.recent_dir_selected, Some(0));
        assert_eq!(app.recent_dir_scroll, 0);
    }

    #[test]
    fn test_recent_dirs_scroll_wraps_to_end() {
        let dirs: Vec<String> = (0..15).map(|i| format!("/dir{i}")).collect();
        let mut app = App::with_recent_dirs(dirs);
        app.focused_field = InputField::Directory;
        app.handle_key(ctrl_d());

        // Wrap from first to last
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.recent_dir_selected, Some(14));
        assert_eq!(app.recent_dir_scroll, 5); // 14 - 10 + 1 = 5
    }

    #[test]
    fn test_recent_dirs_scroll_resets_on_close() {
        let dirs: Vec<String> = (0..15).map(|i| format!("/dir{i}")).collect();
        let mut app = App::with_recent_dirs(dirs);
        app.focused_field = InputField::Directory;
        app.handle_key(ctrl_d());

        for _ in 0..12 {
            app.handle_key(key(KeyCode::Down));
        }
        assert!(app.recent_dir_scroll > 0);

        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.recent_dir_scroll, 0);
    }

    #[test]
    fn test_recent_dirs_scroll_resets_on_select() {
        let dirs: Vec<String> = (0..15).map(|i| format!("/dir{i}")).collect();
        let mut app = App::with_recent_dirs(dirs);
        app.focused_field = InputField::Directory;
        app.handle_key(ctrl_d());

        for _ in 0..12 {
            app.handle_key(key(KeyCode::Down));
        }
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.recent_dir_scroll, 0);
        assert_eq!(app.dir_input.value(), "/dir12");
    }

    #[test]
    fn test_recent_dirs_fuzzy_filter_narrows_list() {
        let dirs = vec![
            "/home/user/projects".to_string(),
            "/home/user/downloads".to_string(),
            "/tmp/sandbox".to_string(),
        ];
        let mut app = App::with_recent_dirs(dirs);
        app.focused_field = InputField::Directory;
        app.handle_key(ctrl_d());

        // Type "proj" — should match only /home/user/projects
        for c in "proj".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        assert_eq!(app.recent_dir_query, "proj");
        assert_eq!(app.filtered_recent_dirs(), vec!["/home/user/projects"]);
        assert_eq!(app.recent_dir_selected, Some(0));
    }

    #[test]
    fn test_recent_dirs_fuzzy_filter_case_insensitive() {
        let dirs = vec!["/home/user/Projects".to_string(), "/tmp".to_string()];
        let mut app = App::with_recent_dirs(dirs);
        app.focused_field = InputField::Directory;
        app.handle_key(ctrl_d());
        for c in "PROJ".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        assert_eq!(app.filtered_recent_dirs(), vec!["/home/user/Projects"]);
    }

    #[test]
    fn test_recent_dirs_fuzzy_backspace_expands_list() {
        let dirs = vec![
            "/home/user/projects".to_string(),
            "/home/user/downloads".to_string(),
        ];
        let mut app = App::with_recent_dirs(dirs);
        app.focused_field = InputField::Directory;
        app.handle_key(ctrl_d());
        for c in "proj".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        assert_eq!(app.filtered_recent_dirs().len(), 1);
        // Backspace back to empty — all dirs visible
        for _ in 0..4 {
            app.handle_key(key(KeyCode::Backspace));
        }
        assert!(app.recent_dir_query.is_empty());
        assert_eq!(app.filtered_recent_dirs().len(), 2);
    }

    #[test]
    fn test_recent_dirs_fuzzy_enter_selects_filtered() {
        let dirs = vec![
            "/home/user/projects".to_string(),
            "/home/user/downloads".to_string(),
        ];
        let mut app = App::with_recent_dirs(dirs);
        app.focused_field = InputField::Directory;
        app.handle_key(ctrl_d());
        for c in "down".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));
        assert!(!app.show_recent_dirs);
        assert_eq!(app.dir_input.value(), "/home/user/downloads");
        assert!(app.recent_dir_query.is_empty());
    }

    #[test]
    fn test_recent_dirs_fuzzy_esc_clears_query() {
        let dirs = vec!["/tmp".to_string()];
        let mut app = App::with_recent_dirs(dirs);
        app.focused_field = InputField::Directory;
        app.handle_key(ctrl_d());
        for c in "foo".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Esc));
        assert!(!app.show_recent_dirs);
        assert!(app.recent_dir_query.is_empty());
    }

    #[test]
    fn test_recent_dirs_fuzzy_no_match_selection_is_none() {
        let dirs = vec!["/home/user/projects".to_string()];
        let mut app = App::with_recent_dirs(dirs);
        app.focused_field = InputField::Directory;
        app.handle_key(ctrl_d());
        for c in "zzz_no_match".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        assert_eq!(app.filtered_recent_dirs().len(), 0);
        assert_eq!(app.recent_dir_selected, None);
    }

    #[test]
    fn test_git_mode_default_is_worktree() {
        let app = App::new();
        assert_eq!(app.git_mode, GitMode::Worktree);
    }

    #[test]
    fn test_git_mode_toggle_with_left_right() {
        let mut app = App::new();
        app.focused_field = InputField::GitMode;

        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.git_mode, GitMode::Branch);

        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.git_mode, GitMode::Worktree);

        app.handle_key(key(KeyCode::Left));
        assert_eq!(app.git_mode, GitMode::Branch);
    }

    #[test]
    fn test_branch_mode_shows_branch_name_field() {
        let mut app = App::new();
        app.show_advanced = true;
        app.focused_field = InputField::GitMode;
        app.handle_key(key(KeyCode::Right)); // Switch to Branch
        assert_eq!(app.git_mode, GitMode::Branch);

        app.handle_key(key(KeyCode::Tab)); // Should go to BranchName
        assert_eq!(app.focused_field, InputField::BranchName);

        app.handle_key(key(KeyCode::Tab)); // Should go to Prompt
        assert_eq!(app.focused_field, InputField::Prompt);
    }

    #[test]
    fn test_worktree_mode_skips_branch_name_field() {
        let mut app = App::new();
        app.show_advanced = true;
        app.focused_field = InputField::GitMode;
        assert_eq!(app.git_mode, GitMode::Worktree);

        app.handle_key(key(KeyCode::Tab)); // Should skip BranchName, go to Prompt
        assert_eq!(app.focused_field, InputField::Prompt);
    }

    #[test]
    fn test_branch_mode_submit_includes_branch_name() {
        let mut app = App::new();
        app.show_advanced = true;
        // Type title
        for ch in "my task".chars() {
            app.handle_key(key(KeyCode::Char(ch)));
        }
        app.handle_key(key(KeyCode::Tab)); // -> Directory
        app.handle_key(key(KeyCode::Tab)); // -> GitMode
        app.handle_key(key(KeyCode::Right)); // Switch to Branch
        app.handle_key(key(KeyCode::Tab)); // -> BranchName
        for ch in "feature/my-branch".chars() {
            app.handle_key(key(KeyCode::Char(ch)));
        }
        let action = app.handle_key(key(KeyCode::Enter));
        match action {
            Action::Submit {
                git_mode,
                branch_name,
                ..
            } => {
                assert_eq!(git_mode, GitMode::Branch);
                assert_eq!(branch_name, Some("feature/my-branch".to_string()));
            }
            _ => panic!("Expected Submit action"),
        }
    }

    #[test]
    fn test_branch_mode_empty_branch_name_shows_error() {
        let mut app = App::new();
        app.show_advanced = true;
        for ch in "my task".chars() {
            app.handle_key(key(KeyCode::Char(ch)));
        }
        app.focused_field = InputField::GitMode;
        app.handle_key(key(KeyCode::Right)); // Switch to Branch
        let action = app.handle_key(key(KeyCode::Enter));
        assert_eq!(action, Action::None);
        assert!(app.error_message.is_some());
        assert!(app.error_message.unwrap().contains("Branch name"));
    }

    #[test]
    fn test_prev_field_from_prompt_goes_to_branch_name_in_branch_mode() {
        let mut app = App::new();
        app.show_advanced = true;
        app.git_mode = GitMode::Branch;
        app.focused_field = InputField::Prompt;
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.focused_field, InputField::BranchName);
    }

    #[test]
    fn test_prev_field_from_prompt_goes_to_git_mode_in_worktree_mode() {
        let mut app = App::new();
        app.show_advanced = true;
        app.git_mode = GitMode::Worktree;
        app.focused_field = InputField::Prompt;
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.focused_field, InputField::GitMode);
    }

    #[test]
    fn test_prev_field_from_prompt_goes_to_directory_in_basic_mode() {
        let mut app = App::new();
        app.focused_field = InputField::Prompt;
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.focused_field, InputField::Directory);
    }

    fn ctrl_a() -> KeyEvent {
        KeyEvent {
            code: KeyCode::Char('a'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn test_ctrl_a_toggles_advanced() {
        let mut app = App::new();
        assert!(!app.show_advanced);
        app.handle_key(ctrl_a());
        assert!(app.show_advanced);
        app.handle_key(ctrl_a());
        assert!(!app.show_advanced);
    }

    #[test]
    fn test_ctrl_a_moves_focus_from_advanced_field() {
        let mut app = App::new();
        app.show_advanced = true;
        app.focused_field = InputField::ClaudeArgs;
        app.handle_key(ctrl_a()); // hide advanced
        assert!(!app.show_advanced);
        assert_eq!(app.focused_field, InputField::Prompt);
    }

    #[test]
    fn test_ctrl_a_moves_focus_from_git_mode() {
        let mut app = App::new();
        app.show_advanced = true;
        app.focused_field = InputField::GitMode;
        app.handle_key(ctrl_a()); // hide advanced
        assert_eq!(app.focused_field, InputField::Prompt);
    }

    #[test]
    fn test_ctrl_a_preserves_basic_field_focus() {
        let mut app = App::new();
        app.focused_field = InputField::Directory;
        app.handle_key(ctrl_a()); // show advanced
        assert!(app.show_advanced);
        assert_eq!(app.focused_field, InputField::Directory);
    }

    #[test]
    fn test_default_show_advanced_is_false() {
        let app = App::new();
        assert!(!app.show_advanced);
    }

    // --- NewTmuxSession mode tests ---

    fn tmux_app() -> App {
        App::with_recent_dirs_and_mode(Vec::new(), FormMode::NewTmuxSession)
    }

    #[test]
    fn test_tmux_mode_default_form_mode() {
        let app = tmux_app();
        assert_eq!(app.form_mode, FormMode::NewTmuxSession);
    }

    #[test]
    fn test_tmux_mode_tab_cycles_title_directory_only() {
        let mut app = tmux_app();
        assert_eq!(app.focused_field, InputField::Title);
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.focused_field, InputField::Directory);
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.focused_field, InputField::Title);
    }

    #[test]
    fn test_tmux_mode_prev_field_cycles_directory_title_only() {
        let mut app = tmux_app();
        assert_eq!(app.focused_field, InputField::Title);
        app.handle_key(key(KeyCode::BackTab));
        assert_eq!(app.focused_field, InputField::Directory);
        app.handle_key(key(KeyCode::BackTab));
        assert_eq!(app.focused_field, InputField::Title);
    }

    #[test]
    fn test_tmux_mode_enter_submits_tmux_session() {
        let mut app = tmux_app();
        for ch in "my-session".chars() {
            app.handle_key(key(KeyCode::Char(ch)));
        }
        app.handle_key(key(KeyCode::Tab));
        while !app.dir_input.value().is_empty() {
            app.handle_key(key(KeyCode::Backspace));
        }
        for ch in "/tmp".chars() {
            app.handle_key(key(KeyCode::Char(ch)));
        }
        let action = app.handle_key(key(KeyCode::Enter));
        assert!(matches!(action, Action::SubmitTmuxSession { .. }));
        if let Action::SubmitTmuxSession { title, directory } = action {
            assert_eq!(title, "my-session");
            assert_eq!(directory, "/tmp");
        }
    }

    #[test]
    fn test_tmux_mode_ctrl_a_does_not_toggle_advanced() {
        let mut app = tmux_app();
        app.handle_key(ctrl_a());
        assert!(!app.show_advanced);
    }
}
