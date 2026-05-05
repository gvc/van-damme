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

use crate::{autocomplete::{BranchCompleter, DirCompleter}, recent_dirs, theme};

fn char_wrap_lines(text: &str, width: usize) -> Vec<String> {
    if width == 0 || text.is_empty() {
        return vec![text.to_string()];
    }
    let chars: Vec<char> = text.chars().collect();
    chars.chunks(width).map(|c| c.iter().collect()).collect()
}

#[derive(Debug)]
pub struct Dropdown {
    pub items: Vec<String>,
    pub selected: Option<usize>,
    pub scroll: usize,
    pub visible: bool,
    pub query: String,
}

impl Dropdown {
    fn new() -> Self {
        Dropdown {
            items: Vec::new(),
            selected: None,
            scroll: 0,
            visible: false,
            query: String::new(),
        }
    }

    pub fn filtered(&self) -> Vec<&str> {
        if self.query.is_empty() {
            self.items.iter().map(|s| s.as_str()).collect()
        } else {
            let q = self.query.to_lowercase();
            self.items
                .iter()
                .filter(|s| s.to_lowercase().contains(&q))
                .map(|s| s.as_str())
                .collect()
        }
    }

    pub fn open(&mut self, items: Vec<String>) {
        self.items = items;
        self.selected = if self.items.is_empty() { None } else { Some(0) };
        self.scroll = 0;
        self.visible = true;
        self.query.clear();
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.selected = None;
        self.scroll = 0;
        self.query.clear();
    }

    pub fn selected_value(&self) -> Option<&str> {
        self.selected
            .and_then(|i| self.filtered().into_iter().nth(i))
    }

    pub fn select_next(&mut self, visible_count: usize) {
        let n = self.filtered().len();
        if n > 0 {
            self.selected = Some(match self.selected {
                Some(i) if i < n - 1 => i + 1,
                _ => 0,
            });
        }
        self.adjust_scroll(visible_count);
    }

    pub fn select_prev(&mut self, visible_count: usize) {
        let n = self.filtered().len();
        if n > 0 {
            self.selected = Some(match self.selected {
                Some(i) if i > 0 => i - 1,
                _ => n - 1,
            });
        }
        self.adjust_scroll(visible_count);
    }

    fn adjust_scroll(&mut self, visible_count: usize) {
        if let Some(sel) = self.selected {
            if sel < self.scroll {
                self.scroll = sel;
            } else if sel >= self.scroll + visible_count {
                self.scroll = sel + 1 - visible_count;
            }
        } else {
            self.scroll = 0;
        }
    }

    pub fn push_query_char(&mut self, c: char) {
        self.query.push(c);
        let n = self.filtered().len();
        self.selected = if n > 0 { Some(0) } else { None };
        self.scroll = 0;
    }

    pub fn pop_query_char(&mut self) {
        self.query.pop();
        let n = self.filtered().len();
        self.selected = if n > 0 { Some(0) } else { None };
        self.scroll = 0;
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModelSelection {
    #[default]
    Default,
    Opus46,
    Sonnet46,
    Haiku45,
}

impl ModelSelection {
    pub const ALL: &'static [ModelSelection] = &[
        ModelSelection::Default,
        ModelSelection::Opus46,
        ModelSelection::Sonnet46,
        ModelSelection::Haiku45,
    ];

    pub fn display_name(self) -> &'static str {
        match self {
            ModelSelection::Default => "default",
            ModelSelection::Opus46 => "opus-4-6",
            ModelSelection::Sonnet46 => "sonnet-4-6",
            ModelSelection::Haiku45 => "haiku-4-5",
        }
    }

    /// Full model ID passed as `--model <id>`. Returns None for Default (no flag emitted).
    pub fn model_id(self) -> Option<&'static str> {
        match self {
            ModelSelection::Default => None,
            ModelSelection::Opus46 => Some("claude-opus-4-6"),
            ModelSelection::Sonnet46 => Some("claude-sonnet-4-6"),
            ModelSelection::Haiku45 => Some("claude-haiku-4-5-20251001"),
        }
    }

    pub fn next(self) -> Self {
        let idx = Self::ALL.iter().position(|&m| m == self).unwrap_or(0);
        Self::ALL[(idx + 1) % Self::ALL.len()]
    }

    pub fn prev(self) -> Self {
        let idx = Self::ALL.iter().position(|&m| m == self).unwrap_or(0);
        Self::ALL[(idx + Self::ALL.len() - 1) % Self::ALL.len()]
    }

    /// Find a ModelSelection from its model_id string. Returns Default for unknown IDs.
    pub fn from_model_id(id: &str) -> Self {
        Self::ALL
            .iter()
            .find(|&&m| m.model_id() == Some(id))
            .copied()
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputField {
    Title,
    Directory,
    BranchName,
    Prompt,
    ClaudeCommand,
    ModelSelection,
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
        claude_command: String,
        model_selection: ModelSelection,
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
    pub claude_command_input: Input,
    pub model_selection: ModelSelection,
    pub dir_suggestion: Option<String>,
    pub branch_suggestion: Option<String>,
    pub error_message: Option<String>,
    pub recent_dirs_dropdown: Dropdown,
    pub branch_dropdown: Dropdown,
}

impl App {
    #[cfg(test)]
    pub fn new() -> Self {
        Self::with_recent_dirs(Vec::new())
    }

    #[cfg(test)]
    pub fn with_recent_dirs(recent_dirs: Vec<String>) -> Self {
        Self::with_recent_dirs_and_mode(recent_dirs, FormMode::NewTask)
    }

    pub fn with_recent_dirs_and_mode(recent_dirs: Vec<String>, form_mode: FormMode) -> Self {
        Self::with_recent_dirs_mode_and_model(recent_dirs, form_mode, None)
    }

    pub fn with_recent_dirs_mode_and_model(
        recent_dirs: Vec<String>,
        form_mode: FormMode,
        last_model: Option<&str>,
    ) -> Self {
        let default_dir = recent_dirs.first().cloned().unwrap_or_else(|| {
            std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default()
        });

        let model_selection = last_model
            .map(ModelSelection::from_model_id)
            .unwrap_or_default();

        let mut recent_dirs_dropdown = Dropdown::new();
        recent_dirs_dropdown.items = recent_dirs;

        Self {
            running: true,
            form_mode,
            focused_field: InputField::Title,
            title_input: Input::default(),
            dir_input: Input::new(default_dir),
            git_mode: GitMode::Worktree,
            branch_name_input: Input::default(),
            prompt_input: Input::default(),
            claude_command_input: Input::new("claude".to_string()),
            model_selection,
            dir_suggestion: None,
            branch_suggestion: None,
            error_message: None,
            recent_dirs_dropdown,
            branch_dropdown: Dropdown::new(),
        }
    }

    pub fn quit(&mut self) {
        self.running = false;
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Action {
        if self.recent_dirs_dropdown.visible {
            return self.handle_recent_dirs_key(key);
        }

        if self.branch_dropdown.visible {
            return self.handle_branch_list_key(key);
        }

        // Ctrl+G toggles git mode (Worktree/Branch) from any field
        if self.form_mode == FormMode::NewTask
            && key.code == KeyCode::Char('g')
            && key
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL)
        {
            self.git_mode = match self.git_mode {
                GitMode::Worktree => GitMode::Branch,
                GitMode::Branch => GitMode::Worktree,
            };
            // If switching away from Branch and BranchName is focused, move to Directory
            if self.git_mode == GitMode::Worktree && self.focused_field == InputField::BranchName {
                self.focused_field = InputField::Directory;
            }
            return Action::None;
        }

        // Ctrl+T toggles between NewTask and NewTmuxSession, preserving title/directory.
        if key.code == KeyCode::Char('t')
            && key
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL)
        {
            self.form_mode = match self.form_mode {
                FormMode::NewTask => FormMode::NewTmuxSession,
                FormMode::NewTmuxSession => FormMode::NewTask,
            };
            // Clamp focused field: NewTmuxSession only has Title and Directory.
            if self.form_mode == FormMode::NewTmuxSession
                && !matches!(
                    self.focused_field,
                    InputField::Title | InputField::Directory
                )
            {
                self.focused_field = InputField::Directory;
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
                    && self.cursor_at_end()
                    && !self.dir_input.value().ends_with('/') =>
            {
                if !self.complete_directory() {
                    self.next_field();
                }
                Action::None
            }
            KeyCode::Tab
                if self.focused_field == InputField::BranchName
                    && self.branch_suggestion.is_some()
                    && self.branch_cursor_at_end() =>
            {
                if !self.complete_branch() {
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
            KeyCode::Right
                if self.focused_field == InputField::BranchName
                    && self.branch_suggestion.is_some()
                    && self.branch_cursor_at_end() =>
            {
                self.complete_branch();
                Action::None
            }
            KeyCode::Left if self.focused_field == InputField::ModelSelection => {
                self.model_selection = self.model_selection.prev();
                Action::None
            }
            KeyCode::Right if self.focused_field == InputField::ModelSelection => {
                self.model_selection = self.model_selection.next();
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
                    && !self.recent_dirs_dropdown.items.is_empty()
                {
                    let items = self.recent_dirs_dropdown.items.clone();
                    self.recent_dirs_dropdown.open(items);
                    return Action::None;
                }

                // Ctrl+D opens branch list when on branch name field
                if self.focused_field == InputField::BranchName
                    && key.code == KeyCode::Char('d')
                    && key
                        .modifiers
                        .contains(crossterm::event::KeyModifiers::CONTROL)
                {
                    let dir = self.dir_input.value().to_string();
                    let branches = crate::git::get_local_branches(&dir);
                    if !branches.is_empty() {
                        self.branch_dropdown.open(branches);
                    }
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
                    InputField::BranchName => {
                        self.branch_name_input
                            .handle_event(&crossterm::event::Event::Key(key));
                        self.update_branch_suggestion();
                    }
                    InputField::Prompt => {
                        self.prompt_input
                            .handle_event(&crossterm::event::Event::Key(key));
                    }
                    InputField::ClaudeCommand => {
                        self.claude_command_input
                            .handle_event(&crossterm::event::Event::Key(key));
                    }
                    InputField::ModelSelection => {
                        // Left/Right handled above; < and > as convenience aliases
                        match key.code {
                            KeyCode::Char('<') => {
                                self.model_selection = self.model_selection.prev();
                            }
                            KeyCode::Char('>') => {
                                self.model_selection = self.model_selection.next();
                            }
                            _ => {}
                        }
                    }
                }
                self.error_message = None;
                Action::None
            }
        }
    }

    const DROPDOWN_VISIBLE: usize = 10;

    fn handle_recent_dirs_key(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Esc => {
                self.recent_dirs_dropdown.close();
                Action::None
            }
            KeyCode::Up | KeyCode::BackTab => {
                self.recent_dirs_dropdown.select_prev(Self::DROPDOWN_VISIBLE);
                Action::None
            }
            KeyCode::Down | KeyCode::Tab => {
                self.recent_dirs_dropdown.select_next(Self::DROPDOWN_VISIBLE);
                Action::None
            }
            KeyCode::Enter => {
                let selected = self.recent_dirs_dropdown.selected_value().map(|s| s.to_string());
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
                self.recent_dirs_dropdown.close();
                Action::None
            }
            KeyCode::Backspace => {
                self.recent_dirs_dropdown.pop_query_char();
                Action::None
            }
            KeyCode::Char(c)
                if !key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                self.recent_dirs_dropdown.push_query_char(c);
                Action::None
            }
            _ => Action::None,
        }
    }

    fn handle_branch_list_key(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Esc => {
                self.branch_dropdown.close();
                Action::None
            }
            KeyCode::Up | KeyCode::BackTab => {
                self.branch_dropdown.select_prev(Self::DROPDOWN_VISIBLE);
                Action::None
            }
            KeyCode::Down | KeyCode::Tab => {
                self.branch_dropdown.select_next(Self::DROPDOWN_VISIBLE);
                Action::None
            }
            KeyCode::Enter => {
                let selected = self.branch_dropdown.selected_value().map(|s| s.to_string());
                if let Some(branch) = selected {
                    self.branch_name_input = Input::new(branch.clone());
                    for _ in 0..branch.len() {
                        self.branch_name_input
                            .handle_event(&crossterm::event::Event::Key(KeyEvent::new(
                                KeyCode::Right,
                                crossterm::event::KeyModifiers::NONE,
                            )));
                    }
                    self.update_branch_suggestion();
                }
                self.branch_dropdown.close();
                Action::None
            }
            KeyCode::Backspace => {
                self.branch_dropdown.pop_query_char();
                Action::None
            }
            KeyCode::Char(c)
                if !key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                self.branch_dropdown.push_query_char(c);
                Action::None
            }
            _ => Action::None,
        }
    }

    fn cursor_at_end(&self) -> bool {
        self.dir_input.visual_cursor() >= self.dir_input.value().len()
    }

    fn complete_directory(&mut self) -> bool {
        let current = self.dir_input.value().to_string();
        if let Some(completed) = DirCompleter.complete(&current) {
            if completed == current {
                return false;
            }
            let len = completed.len();
            self.dir_input = Input::new(completed);
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
        self.dir_suggestion = DirCompleter.suggest(self.dir_input.value());
    }

    fn branch_cursor_at_end(&self) -> bool {
        self.branch_name_input.visual_cursor() >= self.branch_name_input.value().len()
    }

    fn update_branch_suggestion(&mut self) {
        let prefix = self.branch_name_input.value().to_string();
        let dir = self.dir_input.value().to_string();
        self.branch_suggestion = BranchCompleter.suggest(&prefix, &dir);
    }

    fn complete_branch(&mut self) -> bool {
        let prefix = self.branch_name_input.value().to_string();
        let dir = self.dir_input.value().to_string();
        if let Some(common) = BranchCompleter.complete(&prefix, &dir) {
            let len = common.len();
            self.branch_name_input = Input::new(common);
            for _ in 0..len {
                self.branch_name_input
                    .handle_event(&crossterm::event::Event::Key(KeyEvent::new(
                        KeyCode::Right,
                        crossterm::event::KeyModifiers::NONE,
                    )));
            }
            self.update_branch_suggestion();
            true
        } else {
            false
        }
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
                if self.git_mode == GitMode::Branch {
                    InputField::BranchName
                } else {
                    InputField::Prompt
                }
            }
            InputField::BranchName => InputField::Prompt,
            InputField::Prompt => InputField::ClaudeCommand,
            InputField::ClaudeCommand => InputField::ModelSelection,
            InputField::ModelSelection => InputField::Title,
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
            InputField::Title => InputField::ModelSelection,
            InputField::Directory => InputField::Title,
            InputField::BranchName => InputField::Directory,
            InputField::Prompt => {
                if self.git_mode == GitMode::Branch {
                    InputField::BranchName
                } else {
                    InputField::Directory
                }
            }
            InputField::ClaudeCommand => InputField::Prompt,
            InputField::ModelSelection => InputField::ClaudeCommand,
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

        // Create directory if it doesn't exist
        if !Path::new(&directory).is_dir() {
            match std::fs::create_dir_all(&directory) {
                Ok(_) => {
                    let _ = recent_dirs::record_directory(&directory);
                }
                Err(e) => {
                    self.error_message = Some(format!("Cannot create directory: {e}"));
                    return Action::None;
                }
            }
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

        let claude_command = {
            let cmd = self.claude_command_input.value().trim().to_string();
            if cmd.is_empty() {
                "claude".to_string()
            } else {
                cmd
            }
        };

        Action::Submit {
            title,
            directory,
            git_mode: self.git_mode,
            branch_name,
            prompt,
            claude_command,
            model_selection: self.model_selection,
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

        // Branch name field appears when git mode is Branch
        let branch_extra: u16 = if self.git_mode == GitMode::Branch {
            4
        } else {
            0
        };

        // Base: 2 (outer border) + 1 (title label) + 3 (title input) + 1 (dir label) + 3 (dir input)
        //       + branch_extra + 1 (prompt label) + prompt_box_height
        //       + 1 (cmd label) + 3 (cmd input) + 1 (model selector) + 1 (hints)
        let form_height =
            (17 + branch_extra + prompt_box_height).min(area.height.saturating_sub(2));
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

        // Clear area behind form (including error line) and fill with background
        frame.render_widget(Clear, outer_area);
        frame.render_widget(
            Block::default().style(Style::default().bg(theme::BG)),
            outer_area,
        );

        let outer_block = Block::default()
            .title_top(Line::from(Span::styled(
                " New Task ",
                Style::default().fg(theme::ORANGE_BRIGHT),
            )))
            .title_top(
                Line::from(Span::styled(
                    match self.git_mode {
                        GitMode::Worktree => " [git: worktree] ",
                        GitMode::Branch => " [git: branch] ",
                    },
                    Style::default().fg(theme::CYAN),
                ))
                .right_aligned(),
            )
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::ORANGE))
            .style(Style::default().bg(theme::BG));
        let inner = outer_block.inner(form_area);
        frame.render_widget(outer_block, form_area);

        // Build layout constraints
        let mut constraints = vec![
            Constraint::Length(1), // Title label
            Constraint::Length(3), // Title input
            Constraint::Length(1), // Directory label
            Constraint::Length(3), // Directory input
        ];
        if self.git_mode == GitMode::Branch {
            constraints.push(Constraint::Length(1)); // Branch name label
            constraints.push(Constraint::Length(3)); // Branch name input
        }
        constraints.extend_from_slice(&[
            Constraint::Length(1),                 // Prompt label
            Constraint::Length(prompt_box_height), // Prompt input
            Constraint::Length(1),                 // Claude command label
            Constraint::Length(3),                 // Claude command input
            Constraint::Length(1),                 // Model selector widget
            Constraint::Min(1),                    // Hints
        ]);
        let chunks = Layout::vertical(constraints).split(inner);

        // Track chunk indices
        let title_label_idx = 0;
        let title_input_idx = 1;
        let dir_label_idx = 2;
        let dir_input_idx = 3;
        let mut next_idx = 4;

        let branch_label_idx;
        let branch_input_idx;
        if self.git_mode == GitMode::Branch {
            branch_label_idx = Some(next_idx);
            next_idx += 1;
            branch_input_idx = Some(next_idx);
            next_idx += 1;
        } else {
            branch_label_idx = None;
            branch_input_idx = None;
        }

        let prompt_label_idx = next_idx;
        next_idx += 1;
        let prompt_input_idx = next_idx;
        next_idx += 1;
        let cmd_label_idx = next_idx;
        next_idx += 1;
        let cmd_input_idx = next_idx;
        next_idx += 1;
        let model_selector_idx = next_idx;
        next_idx += 1;
        let hints_idx = next_idx;

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

        // Branch name (only when Branch mode)
        let mut branch_inner = None;
        let mut branch_chunk = None;
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
            branch_chunk = Some(chunks[bi_idx]);
            let branch_value = self.branch_name_input.value();
            let branch_line = if let Some(ref suggestion) = self.branch_suggestion {
                Line::from(vec![
                    Span::styled(branch_value.to_string(), Style::default().fg(theme::TEXT)),
                    Span::styled(suggestion.as_str(), Style::default().fg(theme::BLUE)),
                ])
            } else {
                Line::from(Span::styled(
                    branch_value.to_string(),
                    Style::default().fg(theme::TEXT),
                ))
            };
            let branch_para = Paragraph::new(branch_line).block(branch_block);
            frame.render_widget(branch_para, chunks[bi_idx]);
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

        // Claude command label (shows current model beside it)
        let cmd_label_text = if self.model_selection != ModelSelection::Default {
            Line::from(vec![
                Span::styled(
                    "Claude command (default: claude):  ",
                    Style::default().fg(theme::TEXT).bg(theme::BG),
                ),
                Span::styled(
                    format!("[model: {}]", self.model_selection.display_name()),
                    Style::default().fg(theme::CYAN).bg(theme::BG),
                ),
            ])
        } else {
            Line::from(Span::styled(
                "Claude command (default: claude):",
                Style::default().fg(theme::TEXT).bg(theme::BG),
            ))
        };
        let cmd_label = Paragraph::new(cmd_label_text).style(Style::default().bg(theme::BG));
        frame.render_widget(cmd_label, chunks[cmd_label_idx]);

        // Claude command input
        let cmd_border_color = if self.focused_field == InputField::ClaudeCommand {
            theme::ORANGE_BRIGHT
        } else {
            theme::GRAY
        };
        let cmd_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(cmd_border_color))
            .style(Style::default().bg(theme::BG));
        let cmd_inner = cmd_block.inner(chunks[cmd_input_idx]);
        let cmd_para = Paragraph::new(self.claude_command_input.value())
            .style(Style::default().fg(theme::TEXT))
            .block(cmd_block);
        frame.render_widget(cmd_para, chunks[cmd_input_idx]);

        // Model selector widget
        let model_focused = self.focused_field == InputField::ModelSelection;
        let mut spans = vec![Span::styled(
            if model_focused { "< " } else { "  " },
            Style::default().fg(theme::GRAY_DIM).bg(theme::BG),
        )];
        for (i, &variant) in ModelSelection::ALL.iter().enumerate() {
            if i > 0 {
                spans.push(Span::styled("  ", Style::default().bg(theme::BG)));
            }
            let style = if variant == self.model_selection {
                Style::default().fg(theme::BG).bg(theme::ORANGE_BRIGHT)
            } else {
                Style::default().fg(theme::GRAY_DIM).bg(theme::BG)
            };
            spans.push(Span::styled(format!(" {} ", variant.display_name()), style));
        }
        spans.push(Span::styled(
            if model_focused { " >" } else { "  " },
            Style::default().fg(theme::GRAY_DIM).bg(theme::BG),
        ));
        let selector_para = Paragraph::new(Line::from(spans)).style(Style::default().bg(theme::BG));
        frame.render_widget(selector_para, chunks[model_selector_idx]);

        // Hints
        let hint_text = if self.recent_dirs_dropdown.visible || self.branch_dropdown.visible {
            "type to filter  |  ↑/↓: select  |  Enter: confirm  |  Esc: cancel"
        } else if self.focused_field == InputField::ModelSelection {
            "←/→: cycle model  |  Tab: next  |  Ctrl+G: toggle git  |  Ctrl+T: switch mode  |  Enter: submit  |  Esc: quit"
        } else if self.focused_field == InputField::Directory && self.dir_suggestion.is_some() {
            if !self.recent_dirs_dropdown.items.is_empty() {
                "Tab/→: complete  |  Ctrl+D: recent dirs  |  Ctrl+G: toggle git  |  Ctrl+T: switch mode  |  Enter: submit  |  Esc: quit"
            } else {
                "Tab/→: complete  |  Ctrl+G: toggle git  |  Ctrl+T: switch mode  |  Enter: submit  |  Esc: quit"
            }
        } else if self.focused_field == InputField::Directory && !self.recent_dirs_dropdown.items.is_empty() {
            "Ctrl+D: recent dirs  |  Ctrl+G: toggle git  |  Ctrl+T: switch mode  |  Enter: submit  |  Esc: quit"
        } else if self.focused_field == InputField::BranchName && self.branch_suggestion.is_some() {
            "Tab/→: complete  |  Ctrl+D: branches  |  Ctrl+G: toggle git  |  Ctrl+T: switch mode  |  Enter: submit  |  Esc: quit"
        } else if self.focused_field == InputField::BranchName {
            "Ctrl+D: branches  |  Ctrl+G: toggle git  |  Ctrl+T: switch mode  |  Enter: submit  |  Esc: quit"
        } else {
            "Tab: next  |  Ctrl+G: toggle git  |  Ctrl+T: switch mode  |  Enter: submit  |  Esc: quit"
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
        if self.recent_dirs_dropdown.visible && !self.recent_dirs_dropdown.items.is_empty() {
            let filtered = self.recent_dirs_dropdown.filtered();
            let visible = filtered.len().min(Self::DROPDOWN_VISIBLE);
            let dropdown_height = visible as u16 + 2;
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
                .skip(self.recent_dirs_dropdown.scroll)
                .take(visible)
                .map(|(i, d)| {
                    let style = if Some(i) == self.recent_dirs_dropdown.selected {
                        Style::default().fg(theme::TEXT).bg(theme::GRAY)
                    } else {
                        Style::default().fg(theme::GRAY_DIM)
                    };
                    ratatui::widgets::ListItem::new(Line::from(Span::styled(*d, style)))
                })
                .collect();
            let title = if self.recent_dirs_dropdown.query.is_empty() {
                format!(
                    " Recent Directories ({}/{}) ",
                    self.recent_dirs_dropdown.selected.map_or(0, |i| i + 1),
                    filtered.len()
                )
            } else {
                format!(
                    " > {}  ({}/{}) ",
                    self.recent_dirs_dropdown.query,
                    self.recent_dirs_dropdown.selected.map_or(0, |i| i + 1),
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

        // Branch list dropdown (rendered last so it overlays hints/error)
        if self.branch_dropdown.visible
            && !self.branch_dropdown.items.is_empty()
            && let Some(anchor) = branch_chunk
        {
            let filtered = self.branch_dropdown.filtered();
            let visible = filtered.len().min(Self::DROPDOWN_VISIBLE);
            let dropdown_height = visible as u16 + 2;
            let dropdown_area = ratatui::layout::Rect {
                x: anchor.x,
                y: anchor.y + anchor.height,
                width: anchor.width,
                height: dropdown_height,
            };
            frame.render_widget(Clear, dropdown_area);
            let items: Vec<ratatui::widgets::ListItem> = filtered
                .iter()
                .enumerate()
                .skip(self.branch_dropdown.scroll)
                .take(visible)
                .map(|(i, b)| {
                    let style = if Some(i) == self.branch_dropdown.selected {
                        Style::default().fg(theme::TEXT).bg(theme::GRAY)
                    } else {
                        Style::default().fg(theme::GRAY_DIM)
                    };
                    ratatui::widgets::ListItem::new(Line::from(Span::styled(*b, style)))
                })
                .collect();
            let title = if self.branch_dropdown.query.is_empty() {
                format!(
                    " Branches ({}/{}) ",
                    self.branch_dropdown.selected.map_or(0, |i| i + 1),
                    filtered.len()
                )
            } else {
                format!(
                    " > {}  ({}/{}) ",
                    self.branch_dropdown.query,
                    self.branch_dropdown.selected.map_or(0, |i| i + 1),
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
            InputField::ModelSelection => {
                // No cursor for selector field
            }
            _ => {
                let (cursor_input, cursor_area) = match self.focused_field {
                    InputField::Title => (&self.title_input, title_inner),
                    InputField::Directory => (&self.dir_input, dir_inner),
                    InputField::BranchName => {
                        (&self.branch_name_input, branch_inner.unwrap_or(title_inner))
                    }
                    InputField::Prompt => (&self.prompt_input, prompt_inner),
                    InputField::ClaudeCommand => (&self.claude_command_input, cmd_inner),
                    InputField::ModelSelection => unreachable!(),
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

        frame.render_widget(Clear, outer_area);
        frame.render_widget(
            Block::default().style(Style::default().bg(theme::BG)),
            outer_area,
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
        let hint_text = if self.recent_dirs_dropdown.visible {
            "type to filter  |  ↑/↓: select  |  Enter: confirm  |  Esc: cancel"
        } else if self.focused_field == InputField::Directory && self.dir_suggestion.is_some() {
            if !self.recent_dirs_dropdown.items.is_empty() {
                "Tab/→: complete  |  Ctrl+D: recent dirs  |  Ctrl+T: switch mode  |  Enter: submit  |  Esc: back"
            } else {
                "Tab/→: complete  |  Ctrl+T: switch mode  |  Enter: submit  |  Esc: back"
            }
        } else if self.focused_field == InputField::Directory && !self.recent_dirs_dropdown.items.is_empty() {
            "Ctrl+D: recent dirs  |  Ctrl+T: switch mode  |  Enter: submit  |  Esc: back"
        } else {
            "Tab: next  |  Ctrl+T: switch mode  |  Enter: submit  |  Esc: back"
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
        if self.recent_dirs_dropdown.visible && !self.recent_dirs_dropdown.items.is_empty() {
            let filtered = self.recent_dirs_dropdown.filtered();
            let visible = filtered.len().min(Self::DROPDOWN_VISIBLE);
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
                .skip(self.recent_dirs_dropdown.scroll)
                .take(visible)
                .map(|(i, d)| {
                    let style = if Some(i) == self.recent_dirs_dropdown.selected {
                        Style::default().fg(theme::TEXT).bg(theme::GRAY)
                    } else {
                        Style::default().fg(theme::GRAY_DIM)
                    };
                    ratatui::widgets::ListItem::new(Line::from(Span::styled(*d, style)))
                })
                .collect();
            let title = if self.recent_dirs_dropdown.query.is_empty() {
                format!(
                    " Recent Directories ({}/{}) ",
                    self.recent_dirs_dropdown.selected.map_or(0, |i| i + 1),
                    filtered.len()
                )
            } else {
                format!(
                    " > {}  ({}/{}) ",
                    self.recent_dirs_dropdown.query,
                    self.recent_dirs_dropdown.selected.map_or(0, |i| i + 1),
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
    fn test_tab_cycles_all_fields_worktree_mode() {
        let mut app = App::new();
        assert_eq!(app.git_mode, GitMode::Worktree);
        assert_eq!(app.focused_field, InputField::Title);

        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.focused_field, InputField::Directory);

        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.focused_field, InputField::Prompt);

        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.focused_field, InputField::ClaudeCommand);

        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.focused_field, InputField::ModelSelection);

        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.focused_field, InputField::Title);
    }

    #[test]
    fn test_tab_cycles_all_fields_branch_mode() {
        let mut app = App::new();
        app.git_mode = GitMode::Branch;
        assert_eq!(app.focused_field, InputField::Title);

        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.focused_field, InputField::Directory);

        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.focused_field, InputField::BranchName);

        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.focused_field, InputField::Prompt);

        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.focused_field, InputField::ClaudeCommand);

        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.focused_field, InputField::ModelSelection);

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
                claude_command: "claude".to_string(),
                model_selection: ModelSelection::Default,
            }
        );
    }

    #[test]
    fn test_submit_with_nonexistent_directory_creates_it() {
        use tempfile::TempDir;
        // Safety: single-threaded test context; no concurrent env reads.
        unsafe { std::env::set_var("VAN_DAMME_TEST", "1") };

        let tmpdir = TempDir::new().unwrap();
        let nested_path = tmpdir.path().join("new").join("nested").join("dir");
        let nested_str = nested_path.to_string_lossy().to_string();

        let mut app = App::new();
        for ch in "my task".chars() {
            app.handle_key(key(KeyCode::Char(ch)));
        }
        app.handle_key(key(KeyCode::Tab));
        while !app.dir_input.value().is_empty() {
            app.handle_key(key(KeyCode::Backspace));
        }
        for ch in nested_str.chars() {
            app.handle_key(key(KeyCode::Char(ch)));
        }

        // Verify directory doesn't exist yet
        assert!(!nested_path.exists());

        let action = app.handle_key(key(KeyCode::Enter));

        // Should submit (not error)
        assert!(matches!(action, Action::Submit { .. }));
        // Directory should now exist
        assert!(nested_path.exists());
    }

    #[test]
    fn test_typing_clears_error() {
        let mut app = App::new();
        app.error_message = Some("some error".to_string());
        app.handle_key(key(KeyCode::Char('a')));
        assert!(app.error_message.is_none());
    }

    #[test]
    fn test_up_down_cycles_all_fields() {
        let mut app = App::new();
        assert_eq!(app.git_mode, GitMode::Worktree);

        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.focused_field, InputField::Directory);
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.focused_field, InputField::Prompt);
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.focused_field, InputField::ClaudeCommand);
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.focused_field, InputField::ModelSelection);
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.focused_field, InputField::Title);

        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.focused_field, InputField::ModelSelection);
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.focused_field, InputField::ClaudeCommand);
        app.handle_key(key(KeyCode::Up));
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
                git_mode: GitMode::Worktree,
                branch_name: None,
                prompt: Some("fix the bug".to_string()),
                claude_command: "claude".to_string(),
                model_selection: ModelSelection::Default,
            }
        );
    }

    #[test]
    fn test_submit_with_no_prompt_is_none() {
        let mut app = App::new();
        for ch in "my task".chars() {
            app.handle_key(key(KeyCode::Char(ch)));
        }
        let action = app.handle_key(key(KeyCode::Enter));
        match action {
            Action::Submit { prompt, .. } => assert!(prompt.is_none()),
            _ => panic!("Expected Submit"),
        }
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
        assert!(DirCompleter.complete("").is_none());
    }

    #[test]
    fn test_complete_path_completes_tmp() {
        let result = DirCompleter.complete("/tm");
        assert!(result.is_some());
        assert!(result.unwrap().starts_with("/tmp"));
    }

    #[test]
    fn test_complete_path_trailing_slash_lists_children() {
        let _ = DirCompleter.complete("/tmp/");
    }

    #[test]
    fn test_complete_path_nonexistent_returns_none() {
        assert!(DirCompleter.complete("/nonexistent_path_xyz_12345/abc").is_none());
    }

    #[test]
    fn test_longest_common_prefix_single() {
        use crate::autocomplete::longest_common_prefix;
        assert_eq!(longest_common_prefix(&["hello".to_string()]), "hello");
    }

    #[test]
    fn test_longest_common_prefix_multiple() {
        use crate::autocomplete::longest_common_prefix;
        assert_eq!(
            longest_common_prefix(&["hello".to_string(), "help".to_string(), "hero".to_string()]),
            "he"
        );
    }

    #[test]
    fn test_longest_common_prefix_empty_list() {
        use crate::autocomplete::longest_common_prefix;
        let empty: Vec<String> = vec![];
        assert_eq!(longest_common_prefix(&empty), "");
    }

    #[test]
    fn test_longest_common_prefix_identical() {
        use crate::autocomplete::longest_common_prefix;
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
    fn test_tab_at_trailing_slash_moves_to_next_field() {
        // When input ends with '/' (user is already at a complete dir path),
        // Tab must move to next field regardless of dir_suggestion.
        let mut app = App::new();
        app.focused_field = InputField::Directory;
        // /tmp/ is a valid dir with children — dir_suggestion will be set
        app.dir_input = Input::new("/tmp/".to_string());
        for _ in 0..6 {
            app.dir_input
                .handle_event(&crossterm::event::Event::Key(key(KeyCode::Right)));
        }
        app.update_dir_suggestion();
        // Even if suggestion exists, Tab at trailing '/' must go to next field
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.focused_field, InputField::Prompt);
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
    fn test_tab_on_directory_moves_to_prompt_in_worktree_mode() {
        let mut app = App::new();
        app.focused_field = InputField::Directory;
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.focused_field, InputField::Prompt);
    }

    #[test]
    fn test_tab_on_directory_moves_to_branch_name_in_branch_mode() {
        let mut app = App::new();
        app.git_mode = GitMode::Branch;
        app.focused_field = InputField::Directory;
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.focused_field, InputField::BranchName);
    }

    #[test]
    fn test_with_recent_dirs_defaults_to_most_recent() {
        let dirs = vec!["/home/user/project".to_string(), "/tmp".to_string()];
        let app = App::with_recent_dirs(dirs);
        assert_eq!(app.dir_input.value(), "/home/user/project");
        assert_eq!(app.recent_dirs_dropdown.items.len(), 2);
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
        assert!(app.recent_dirs_dropdown.visible);
        assert_eq!(app.recent_dirs_dropdown.selected, Some(0));
    }

    #[test]
    fn test_ctrl_d_noop_without_recent_dirs() {
        let mut app = App::new();
        app.focused_field = InputField::Directory;
        app.handle_key(ctrl_d());
        assert!(!app.recent_dirs_dropdown.visible);
    }

    #[test]
    fn test_ctrl_d_noop_on_other_fields() {
        let dirs = vec!["/tmp".to_string()];
        let mut app = App::with_recent_dirs(dirs);
        app.focused_field = InputField::Title;
        app.handle_key(ctrl_d());
        assert!(!app.recent_dirs_dropdown.visible);
    }

    #[test]
    fn test_recent_dirs_navigate_down_and_up() {
        let dirs = vec!["/a".to_string(), "/b".to_string(), "/c".to_string()];
        let mut app = App::with_recent_dirs(dirs);
        app.focused_field = InputField::Directory;
        app.handle_key(ctrl_d());

        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.recent_dirs_dropdown.selected, Some(1));

        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.recent_dirs_dropdown.selected, Some(2));

        // Wraps around
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.recent_dirs_dropdown.selected, Some(0));

        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.recent_dirs_dropdown.selected, Some(2));
    }

    #[test]
    fn test_recent_dirs_enter_selects() {
        let dirs = vec!["/tmp".to_string(), "/home".to_string()];
        let mut app = App::with_recent_dirs(dirs);
        app.focused_field = InputField::Directory;
        app.handle_key(ctrl_d());
        app.handle_key(key(KeyCode::Down)); // select /home
        app.handle_key(key(KeyCode::Enter));

        assert!(!app.recent_dirs_dropdown.visible);
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

        assert!(!app.recent_dirs_dropdown.visible);
        assert_eq!(app.dir_input.value(), original_dir);
    }

    #[test]
    fn test_recent_dirs_scroll_adjusts_on_navigate_down() {
        // Create more dirs than RECENT_DIRS_VISIBLE (10)
        let dirs: Vec<String> = (0..15).map(|i| format!("/dir{i}")).collect();
        let mut app = App::with_recent_dirs(dirs);
        app.focused_field = InputField::Directory;
        app.handle_key(ctrl_d());
        assert_eq!(app.recent_dirs_dropdown.scroll, 0);

        // Navigate down past the visible window
        for _ in 0..10 {
            app.handle_key(key(KeyCode::Down));
        }
        assert_eq!(app.recent_dirs_dropdown.selected, Some(10));
        assert_eq!(app.recent_dirs_dropdown.scroll, 1); // scrolled to keep selection visible

        // Navigate back up — still in view
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.recent_dirs_dropdown.selected, Some(9));
        assert_eq!(app.recent_dirs_dropdown.scroll, 1);

        // Navigate up past scroll offset
        for _ in 0..9 {
            app.handle_key(key(KeyCode::Up));
        }
        assert_eq!(app.recent_dirs_dropdown.selected, Some(0));
        assert_eq!(app.recent_dirs_dropdown.scroll, 0);
    }

    #[test]
    fn test_recent_dirs_scroll_wraps_to_end() {
        let dirs: Vec<String> = (0..15).map(|i| format!("/dir{i}")).collect();
        let mut app = App::with_recent_dirs(dirs);
        app.focused_field = InputField::Directory;
        app.handle_key(ctrl_d());

        // Wrap from first to last
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.recent_dirs_dropdown.selected, Some(14));
        assert_eq!(app.recent_dirs_dropdown.scroll, 5); // 14 - 10 + 1 = 5
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
        assert!(app.recent_dirs_dropdown.scroll > 0);

        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.recent_dirs_dropdown.scroll, 0);
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
        assert_eq!(app.recent_dirs_dropdown.scroll, 0);
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
        assert_eq!(app.recent_dirs_dropdown.query, "proj");
        assert_eq!(app.recent_dirs_dropdown.filtered(), vec!["/home/user/projects"]);
        assert_eq!(app.recent_dirs_dropdown.selected, Some(0));
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
        assert_eq!(app.recent_dirs_dropdown.filtered(), vec!["/home/user/Projects"]);
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
        assert_eq!(app.recent_dirs_dropdown.filtered().len(), 1);
        // Backspace back to empty — all dirs visible
        for _ in 0..4 {
            app.handle_key(key(KeyCode::Backspace));
        }
        assert!(app.recent_dirs_dropdown.query.is_empty());
        assert_eq!(app.recent_dirs_dropdown.filtered().len(), 2);
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
        assert!(!app.recent_dirs_dropdown.visible);
        assert_eq!(app.dir_input.value(), "/home/user/downloads");
        assert!(app.recent_dirs_dropdown.query.is_empty());
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
        assert!(!app.recent_dirs_dropdown.visible);
        assert!(app.recent_dirs_dropdown.query.is_empty());
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
        assert_eq!(app.recent_dirs_dropdown.filtered().len(), 0);
        assert_eq!(app.recent_dirs_dropdown.selected, None);
    }

    #[test]
    fn test_git_mode_default_is_worktree() {
        let app = App::new();
        assert_eq!(app.git_mode, GitMode::Worktree);
    }

    fn ctrl_g() -> KeyEvent {
        KeyEvent {
            code: KeyCode::Char('g'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn ctrl_t() -> KeyEvent {
        KeyEvent {
            code: KeyCode::Char('t'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn test_ctrl_g_toggles_git_mode_from_any_field() {
        let mut app = App::new();
        assert_eq!(app.git_mode, GitMode::Worktree);
        app.handle_key(ctrl_g());
        assert_eq!(app.git_mode, GitMode::Branch);
        app.handle_key(ctrl_g());
        assert_eq!(app.git_mode, GitMode::Worktree);
    }

    #[test]
    fn test_ctrl_g_from_branch_name_moves_focus_to_directory_when_switching_to_worktree() {
        let mut app = App::new();
        app.git_mode = GitMode::Branch;
        app.focused_field = InputField::BranchName;
        app.handle_key(ctrl_g()); // switch to Worktree
        assert_eq!(app.git_mode, GitMode::Worktree);
        assert_eq!(app.focused_field, InputField::Directory);
    }

    #[test]
    fn test_branch_mode_shows_branch_name_in_tab_cycle() {
        let mut app = App::new();
        app.git_mode = GitMode::Branch;
        app.focused_field = InputField::Directory;
        app.handle_key(key(KeyCode::Tab)); // -> BranchName
        assert_eq!(app.focused_field, InputField::BranchName);
        app.handle_key(key(KeyCode::Tab)); // -> Prompt
        assert_eq!(app.focused_field, InputField::Prompt);
    }

    #[test]
    fn test_worktree_mode_skips_branch_name_field() {
        let mut app = App::new();
        assert_eq!(app.git_mode, GitMode::Worktree);
        app.focused_field = InputField::Directory;
        app.handle_key(key(KeyCode::Tab)); // Should skip BranchName, go to Prompt
        assert_eq!(app.focused_field, InputField::Prompt);
    }

    #[test]
    fn test_branch_mode_submit_includes_branch_name() {
        let mut app = App::new();
        app.git_mode = GitMode::Branch;
        for ch in "my task".chars() {
            app.handle_key(key(KeyCode::Char(ch)));
        }
        app.handle_key(key(KeyCode::Tab)); // -> Directory
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
        app.git_mode = GitMode::Branch;
        for ch in "my task".chars() {
            app.handle_key(key(KeyCode::Char(ch)));
        }
        let action = app.handle_key(key(KeyCode::Enter));
        assert_eq!(action, Action::None);
        assert!(app.error_message.is_some());
        assert!(app.error_message.unwrap().contains("Branch name"));
    }

    #[test]
    fn test_prev_field_from_prompt_goes_to_branch_name_in_branch_mode() {
        let mut app = App::new();
        app.git_mode = GitMode::Branch;
        app.focused_field = InputField::Prompt;
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.focused_field, InputField::BranchName);
    }

    #[test]
    fn test_prev_field_from_prompt_goes_to_directory_in_worktree_mode() {
        let mut app = App::new();
        assert_eq!(app.git_mode, GitMode::Worktree);
        app.focused_field = InputField::Prompt;
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.focused_field, InputField::Directory);
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
    fn test_tmux_mode_ctrl_g_does_not_change_form_mode() {
        let mut app = tmux_app();
        // Ctrl+G should not crash and form_mode unchanged
        app.handle_key(ctrl_g());
        assert_eq!(app.form_mode, FormMode::NewTmuxSession);
    }

    #[test]
    fn test_ctrl_t_toggles_from_new_task_to_tmux() {
        let mut app = App::new();
        assert_eq!(app.form_mode, FormMode::NewTask);
        app.handle_key(ctrl_t());
        assert_eq!(app.form_mode, FormMode::NewTmuxSession);
    }

    #[test]
    fn test_ctrl_t_toggles_back_to_new_task() {
        let mut app = tmux_app();
        assert_eq!(app.form_mode, FormMode::NewTmuxSession);
        app.handle_key(ctrl_t());
        assert_eq!(app.form_mode, FormMode::NewTask);
    }

    #[test]
    fn test_ctrl_t_preserves_title_and_directory() {
        let mut app = App::new();
        for ch in "my task".chars() {
            app.handle_key(key(KeyCode::Char(ch)));
        }
        app.handle_key(key(KeyCode::Tab));
        while !app.dir_input.value().is_empty() {
            app.handle_key(key(KeyCode::Backspace));
        }
        for ch in "/home/user".chars() {
            app.handle_key(key(KeyCode::Char(ch)));
        }
        app.handle_key(ctrl_t());
        assert_eq!(app.form_mode, FormMode::NewTmuxSession);
        assert_eq!(app.title_input.value(), "my task");
        assert_eq!(app.dir_input.value(), "/home/user");
    }

    #[test]
    fn test_ctrl_t_clamps_focused_field_to_directory_when_on_prompt() {
        let mut app = App::new();
        // Navigate to Prompt field
        app.focused_field = InputField::Prompt;
        app.handle_key(ctrl_t());
        assert_eq!(app.form_mode, FormMode::NewTmuxSession);
        // Prompt doesn't exist in tmux mode — should clamp to Directory
        assert_eq!(app.focused_field, InputField::Directory);
    }

    #[test]
    fn test_ctrl_t_preserves_title_field_when_switching() {
        let mut app = App::new();
        // Title field is valid in both modes — should stay
        assert_eq!(app.focused_field, InputField::Title);
        app.handle_key(ctrl_t());
        assert_eq!(app.focused_field, InputField::Title);
    }

    // --- ModelSelection tests ---

    #[test]
    fn test_model_selection_default_on_new_app() {
        let app = App::new();
        assert_eq!(app.model_selection, ModelSelection::Default);
    }

    #[test]
    fn test_model_selection_display_names() {
        assert_eq!(ModelSelection::Default.display_name(), "default");
        assert_eq!(ModelSelection::Opus46.display_name(), "opus-4-6");
        assert_eq!(ModelSelection::Sonnet46.display_name(), "sonnet-4-6");
        assert_eq!(ModelSelection::Haiku45.display_name(), "haiku-4-5");
    }

    #[test]
    fn test_model_selection_model_ids() {
        assert_eq!(ModelSelection::Default.model_id(), None);
        assert_eq!(ModelSelection::Opus46.model_id(), Some("claude-opus-4-6"));
        assert_eq!(
            ModelSelection::Sonnet46.model_id(),
            Some("claude-sonnet-4-6")
        );
        assert_eq!(
            ModelSelection::Haiku45.model_id(),
            Some("claude-haiku-4-5-20251001")
        );
    }

    #[test]
    fn test_model_selection_next_wraps() {
        let mut m = ModelSelection::Default;
        m = m.next();
        assert_eq!(m, ModelSelection::Opus46);
        m = m.next();
        assert_eq!(m, ModelSelection::Sonnet46);
        m = m.next();
        assert_eq!(m, ModelSelection::Haiku45);
        m = m.next();
        assert_eq!(m, ModelSelection::Default); // wraps
    }

    #[test]
    fn test_model_selection_prev_wraps() {
        let mut m = ModelSelection::Default;
        m = m.prev();
        assert_eq!(m, ModelSelection::Haiku45); // wraps
        m = m.prev();
        assert_eq!(m, ModelSelection::Sonnet46);
        m = m.prev();
        assert_eq!(m, ModelSelection::Opus46);
        m = m.prev();
        assert_eq!(m, ModelSelection::Default);
    }

    #[test]
    fn test_model_selection_from_model_id_known() {
        assert_eq!(
            ModelSelection::from_model_id("claude-opus-4-6"),
            ModelSelection::Opus46
        );
        assert_eq!(
            ModelSelection::from_model_id("claude-sonnet-4-6"),
            ModelSelection::Sonnet46
        );
        assert_eq!(
            ModelSelection::from_model_id("claude-haiku-4-5-20251001"),
            ModelSelection::Haiku45
        );
    }

    #[test]
    fn test_model_selection_from_model_id_unknown_falls_back() {
        assert_eq!(
            ModelSelection::from_model_id("unknown-model"),
            ModelSelection::Default
        );
    }

    #[test]
    fn test_model_selection_with_last_model_sets_correct_variant() {
        let app = App::with_recent_dirs_mode_and_model(
            Vec::new(),
            FormMode::NewTask,
            Some("claude-sonnet-4-6"),
        );
        assert_eq!(app.model_selection, ModelSelection::Sonnet46);
    }

    #[test]
    fn test_model_selection_with_none_last_model_defaults() {
        let app = App::with_recent_dirs_mode_and_model(Vec::new(), FormMode::NewTask, None);
        assert_eq!(app.model_selection, ModelSelection::Default);
    }

    #[test]
    fn test_right_arrow_cycles_model_selection_forward() {
        let mut app = App::new();
        app.focused_field = InputField::ModelSelection;
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.model_selection, ModelSelection::Opus46);
        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.model_selection, ModelSelection::Sonnet46);
    }

    #[test]
    fn test_left_arrow_cycles_model_selection_backward() {
        let mut app = App::new();
        app.focused_field = InputField::ModelSelection;
        app.handle_key(key(KeyCode::Left));
        assert_eq!(app.model_selection, ModelSelection::Haiku45);
    }

    #[test]
    fn test_submit_carries_model_selection() {
        let mut app = App::new();
        app.model_selection = ModelSelection::Sonnet46;
        for ch in "my task".chars() {
            app.handle_key(key(KeyCode::Char(ch)));
        }
        let action = app.handle_key(key(KeyCode::Enter));
        match action {
            Action::Submit {
                model_selection, ..
            } => {
                assert_eq!(model_selection, ModelSelection::Sonnet46);
            }
            _ => panic!("Expected Submit action"),
        }
    }

    #[test]
    fn test_tab_claude_command_goes_to_model_selection() {
        let mut app = App::new();
        app.focused_field = InputField::ClaudeCommand;
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.focused_field, InputField::ModelSelection);
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.focused_field, InputField::Title);
    }

    #[test]
    fn test_prev_field_from_title_goes_to_model_selection() {
        let mut app = App::new();
        app.focused_field = InputField::Title;
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.focused_field, InputField::ModelSelection);
    }

    fn init_test_repo_with_branches(dir: &std::path::Path, branches: &[&str]) {
        use std::process::Command;
        Command::new("git")
            .args(["init"])
            .current_dir(dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(dir)
            .output()
            .unwrap();
        std::fs::write(dir.join("README.md"), "init").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["branch", "-M", "main"])
            .current_dir(dir)
            .output()
            .unwrap();
        for branch in branches {
            Command::new("git")
                .args(["branch", branch])
                .current_dir(dir)
                .output()
                .unwrap();
        }
    }

    #[test]
    fn test_update_branch_suggestion_shows_suffix() {
        let tmp = tempfile::tempdir().unwrap();
        init_test_repo_with_branches(tmp.path(), &["feature-auth", "feature-ui"]);
        let dir = tmp.path().to_str().unwrap().to_string();

        let mut app = App::new();
        app.git_mode = GitMode::Branch;
        app.focused_field = InputField::BranchName;
        app.dir_input = Input::new(dir);
        app.branch_name_input = Input::new("feature-a".to_string());
        app.update_branch_suggestion();

        assert_eq!(app.branch_suggestion, Some("uth".to_string()));
    }

    #[test]
    fn test_update_branch_suggestion_no_match() {
        let tmp = tempfile::tempdir().unwrap();
        init_test_repo_with_branches(tmp.path(), &["feature-auth"]);
        let dir = tmp.path().to_str().unwrap().to_string();

        let mut app = App::new();
        app.dir_input = Input::new(dir);
        app.branch_name_input = Input::new("xyz".to_string());
        app.update_branch_suggestion();

        assert_eq!(app.branch_suggestion, None);
    }

    #[test]
    fn test_update_branch_suggestion_exact_match_no_suggestion() {
        let tmp = tempfile::tempdir().unwrap();
        init_test_repo_with_branches(tmp.path(), &["feature-auth"]);
        let dir = tmp.path().to_str().unwrap().to_string();

        let mut app = App::new();
        app.dir_input = Input::new(dir);
        app.branch_name_input = Input::new("feature-auth".to_string());
        app.update_branch_suggestion();

        assert_eq!(app.branch_suggestion, None);
    }

    #[test]
    fn test_complete_branch_fills_common_prefix() {
        let tmp = tempfile::tempdir().unwrap();
        init_test_repo_with_branches(tmp.path(), &["feature-auth", "feature-api"]);
        let dir = tmp.path().to_str().unwrap().to_string();

        let mut app = App::new();
        app.git_mode = GitMode::Branch;
        app.focused_field = InputField::BranchName;
        app.dir_input = Input::new(dir);
        app.branch_name_input = Input::new("feat".to_string());
        // Move cursor to end
        for _ in 0.."feat".len() {
            app.branch_name_input
                .handle_event(&crossterm::event::Event::Key(key(KeyCode::Right)));
        }
        app.update_branch_suggestion();

        let progressed = app.complete_branch();
        assert!(progressed);
        assert_eq!(app.branch_name_input.value(), "feature-a");
    }

    #[test]
    fn test_complete_branch_single_match_fills_full() {
        let tmp = tempfile::tempdir().unwrap();
        init_test_repo_with_branches(tmp.path(), &["feature-auth"]);
        let dir = tmp.path().to_str().unwrap().to_string();

        let mut app = App::new();
        app.git_mode = GitMode::Branch;
        app.focused_field = InputField::BranchName;
        app.dir_input = Input::new(dir);
        app.branch_name_input = Input::new("feat".to_string());
        for _ in 0.."feat".len() {
            app.branch_name_input
                .handle_event(&crossterm::event::Event::Key(key(KeyCode::Right)));
        }
        app.update_branch_suggestion();

        let progressed = app.complete_branch();
        assert!(progressed);
        assert_eq!(app.branch_name_input.value(), "feature-auth");
    }

    #[test]
    fn test_tab_on_branch_field_completes() {
        let tmp = tempfile::tempdir().unwrap();
        init_test_repo_with_branches(tmp.path(), &["feature-auth"]);
        let dir = tmp.path().to_str().unwrap().to_string();

        let mut app = App::new();
        app.git_mode = GitMode::Branch;
        app.focused_field = InputField::BranchName;
        app.dir_input = Input::new(dir);
        // Type "feat" into branch field
        for c in "feat".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        // suggestion should be active
        assert!(app.branch_suggestion.is_some());

        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.branch_name_input.value(), "feature-auth");
        // still on BranchName (progress was made)
        assert_eq!(app.focused_field, InputField::BranchName);
    }

    #[test]
    fn test_right_on_branch_field_completes() {
        let tmp = tempfile::tempdir().unwrap();
        init_test_repo_with_branches(tmp.path(), &["feature-auth"]);
        let dir = tmp.path().to_str().unwrap().to_string();

        let mut app = App::new();
        app.git_mode = GitMode::Branch;
        app.focused_field = InputField::BranchName;
        app.dir_input = Input::new(dir);
        for c in "feat".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        assert!(app.branch_suggestion.is_some());

        app.handle_key(key(KeyCode::Right));
        assert_eq!(app.branch_name_input.value(), "feature-auth");
    }

    fn ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: crossterm::event::KeyModifiers::CONTROL,
            kind: crossterm::event::KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        }
    }

    #[test]
    fn test_ctrl_d_on_branch_field_opens_list() {
        let tmp = tempfile::tempdir().unwrap();
        init_test_repo_with_branches(tmp.path(), &["feature-auth", "feature-ui"]);
        let dir = tmp.path().to_str().unwrap().to_string();

        let mut app = App::new();
        app.git_mode = GitMode::Branch;
        app.focused_field = InputField::BranchName;
        app.dir_input = Input::new(dir);

        app.handle_key(ctrl(KeyCode::Char('d')));

        assert!(app.branch_dropdown.visible);
        assert_eq!(app.branch_dropdown.selected, Some(0));
        // main + feature-auth + feature-ui = 3 branches
        assert!(app.branch_dropdown.items.len() >= 2);
    }

    #[test]
    fn test_ctrl_d_no_git_repo_does_not_open_list() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_str().unwrap().to_string();

        let mut app = App::new();
        app.git_mode = GitMode::Branch;
        app.focused_field = InputField::BranchName;
        app.dir_input = Input::new(dir);

        app.handle_key(ctrl(KeyCode::Char('d')));

        assert!(!app.branch_dropdown.visible);
    }

    #[test]
    fn test_branch_list_filter_by_typing() {
        let tmp = tempfile::tempdir().unwrap();
        init_test_repo_with_branches(tmp.path(), &["feature-auth", "fix-bug"]);
        let dir = tmp.path().to_str().unwrap().to_string();

        let mut app = App::new();
        app.git_mode = GitMode::Branch;
        app.focused_field = InputField::BranchName;
        app.dir_input = Input::new(dir);
        app.handle_key(ctrl(KeyCode::Char('d')));
        assert!(app.branch_dropdown.visible);

        // type "feat" to filter
        for c in "feat".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        let filtered = app.branch_dropdown.filtered();
        assert!(filtered.iter().all(|b| b.contains("feat")));
    }

    #[test]
    fn test_branch_list_enter_selects_branch() {
        let tmp = tempfile::tempdir().unwrap();
        init_test_repo_with_branches(tmp.path(), &["feature-auth"]);
        let dir = tmp.path().to_str().unwrap().to_string();

        let mut app = App::new();
        app.git_mode = GitMode::Branch;
        app.focused_field = InputField::BranchName;
        app.dir_input = Input::new(dir);
        app.handle_key(ctrl(KeyCode::Char('d')));

        // Filter down to feature-auth
        for c in "feature".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        app.handle_key(key(KeyCode::Enter));

        assert!(!app.branch_dropdown.visible);
        assert_eq!(app.branch_name_input.value(), "feature-auth");
    }

    #[test]
    fn test_branch_list_esc_closes_without_change() {
        let tmp = tempfile::tempdir().unwrap();
        init_test_repo_with_branches(tmp.path(), &["feature-auth"]);
        let dir = tmp.path().to_str().unwrap().to_string();

        let mut app = App::new();
        app.git_mode = GitMode::Branch;
        app.focused_field = InputField::BranchName;
        app.dir_input = Input::new(dir);
        app.handle_key(ctrl(KeyCode::Char('d')));
        assert!(app.branch_dropdown.visible);

        app.handle_key(key(KeyCode::Esc));
        assert!(!app.branch_dropdown.visible);
        assert_eq!(app.branch_name_input.value(), "");
    }
}
