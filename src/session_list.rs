use std::collections::BTreeMap;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Flex, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
};

use crate::session::{SessionRecord, SessionState};
use crate::theme;
use crate::tmux;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionListAction {
    None,
    Quit,
    NewTask,
    NewTmuxSession,
    Attach { session_name: String },
}

/// A row in the display list — either a group header or a selectable session.
#[derive(Debug, Clone)]
enum DisplayRow {
    /// Non-selectable directory group header.
    GroupHeader(String),
    /// Selectable session row. Stores index into `self.sessions`.
    Session(usize),
}

#[derive(Debug)]
pub struct SessionList {
    pub sessions: Vec<SessionRecord>,
    /// Display rows (headers + session items), rebuilt on refresh.
    display_rows: Vec<DisplayRow>,
    pub list_state: ListState,
    pub status_message: Option<String>,
    /// When set, we're waiting for the user to confirm killing the session at this index.
    confirm_kill: Option<usize>,
}

impl SessionList {
    pub fn new(sessions: Vec<SessionRecord>) -> Self {
        let display_rows = Self::build_display_rows(&sessions);
        let mut list_state = ListState::default();
        // Select first selectable row (skip headers)
        let first_selectable = display_rows
            .iter()
            .position(|r| matches!(r, DisplayRow::Session(_)));
        list_state.select(first_selectable);
        Self {
            sessions,
            display_rows,
            list_state,
            status_message: None,
            confirm_kill: None,
        }
    }

    /// Build display rows: group sessions by directory, sorted alphabetically.
    /// Within each group, sessions appear in their original order (creation time).
    fn build_display_rows(sessions: &[SessionRecord]) -> Vec<DisplayRow> {
        if sessions.is_empty() {
            return vec![];
        }

        // Group session indices by directory, preserving order within groups.
        let mut groups: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
        for (i, s) in sessions.iter().enumerate() {
            groups.entry(&s.directory).or_default().push(i);
        }

        let mut rows = Vec::new();
        for (dir, indices) in &groups {
            if !rows.is_empty() {
                // Blank separator rendered as an empty non-selectable header
                rows.push(DisplayRow::GroupHeader(String::new()));
            }
            rows.push(DisplayRow::GroupHeader(dir.to_string()));
            for &idx in indices {
                rows.push(DisplayRow::Session(idx));
            }
        }
        rows
    }

    fn rebuild_display_rows(&mut self) {
        self.display_rows = Self::build_display_rows(&self.sessions);
    }

    /// Map the currently selected display row to a session index, if it's a session row.
    fn selected_session_index(&self) -> Option<usize> {
        let row_idx = self.list_state.selected()?;
        match self.display_rows.get(row_idx) {
            Some(DisplayRow::Session(session_idx)) => Some(*session_idx),
            _ => None,
        }
    }

    /// Find selectable row indices.
    fn selectable_indices(&self) -> Vec<usize> {
        self.display_rows
            .iter()
            .enumerate()
            .filter_map(|(i, r)| match r {
                DisplayRow::Session(_) => Some(i),
                _ => None,
            })
            .collect()
    }

    pub fn refresh(&mut self) {
        match crate::session::list_sessions() {
            Ok(sessions) => {
                // Filter to only sessions that are still alive in tmux
                let alive: Vec<SessionRecord> = sessions
                    .into_iter()
                    .filter(|s| tmux::session_exists(&s.tmux_session_name).unwrap_or(false))
                    .collect();
                self.sessions = alive;
                self.rebuild_display_rows();
                self.clamp_selection();
            }
            Err(e) => {
                self.status_message = Some(format!("Error loading sessions: {e}"));
            }
        }
    }

    /// Lightweight refresh: re-reads session states from the DB without
    /// spawning tmux processes to check liveness. Suitable for calling on tick.
    pub fn refresh_states(&mut self) {
        let Ok(db_sessions) = crate::session::list_sessions() else {
            return;
        };
        for session in &mut self.sessions {
            if let Some(updated) = db_sessions
                .iter()
                .find(|s| s.tmux_session_name == session.tmux_session_name)
            {
                session.state = updated.state.clone();
            }
        }
    }

    /// Select a session by tmux name. Falls back to first selectable row if not found.
    pub fn select_by_name(&mut self, name: &str) {
        // Find the session index for this name
        let session_idx = self
            .sessions
            .iter()
            .position(|s| s.tmux_session_name == name);

        // Find the display row that maps to that session index
        let display_idx = session_idx.and_then(|si| {
            self.display_rows
                .iter()
                .position(|r| matches!(r, DisplayRow::Session(idx) if *idx == si))
        });

        let fallback = self
            .display_rows
            .iter()
            .position(|r| matches!(r, DisplayRow::Session(_)));

        let target = display_idx.or(fallback);
        self.list_state.select(target);
    }

    fn clamp_selection(&mut self) {
        let selectable = self.selectable_indices();
        if selectable.is_empty() {
            self.list_state.select(None);
        } else if self.list_state.selected().is_none()
            || !selectable.contains(&self.list_state.selected().unwrap())
        {
            self.list_state.select(Some(selectable[0]));
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> SessionListAction {
        // If we're waiting for kill confirmation, handle that first
        if let Some(idx) = self.confirm_kill {
            self.confirm_kill = None;
            match key.code {
                KeyCode::Char('x') | KeyCode::Char('y') => {
                    self.kill_session_at(idx);
                }
                _ => {
                    self.status_message = Some("Kill cancelled.".to_string());
                }
            }
            return SessionListAction::None;
        }

        match key.code {
            KeyCode::Char('q') => SessionListAction::Quit,
            KeyCode::Char('n') => SessionListAction::NewTask,
            KeyCode::Char('t') => SessionListAction::NewTmuxSession,
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_up();
                SessionListAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_down();
                SessionListAction::None
            }
            KeyCode::Enter | KeyCode::Char('a') => self.attach_selected(),
            KeyCode::Char('x') => {
                self.request_kill_selected();
                SessionListAction::None
            }
            _ => SessionListAction::None,
        }
    }

    fn move_up(&mut self) {
        let selectable = self.selectable_indices();
        if selectable.is_empty() {
            return;
        }
        let current = self.list_state.selected();
        let next = match current {
            Some(i) => {
                // Find current position in selectable list, move to previous (wrapping)
                let pos = selectable.iter().position(|&s| s == i).unwrap_or(0);
                if pos == 0 {
                    *selectable.last().unwrap()
                } else {
                    selectable[pos - 1]
                }
            }
            None => selectable[0],
        };
        self.list_state.select(Some(next));
    }

    fn move_down(&mut self) {
        let selectable = self.selectable_indices();
        if selectable.is_empty() {
            return;
        }
        let current = self.list_state.selected();
        let next = match current {
            Some(i) => {
                let pos = selectable.iter().position(|&s| s == i).unwrap_or(0);
                if pos >= selectable.len() - 1 {
                    selectable[0]
                } else {
                    selectable[pos + 1]
                }
            }
            None => selectable[0],
        };
        self.list_state.select(Some(next));
    }

    fn attach_selected(&self) -> SessionListAction {
        if let Some(session_idx) = self.selected_session_index() {
            let session = &self.sessions[session_idx];
            SessionListAction::Attach {
                session_name: session.tmux_session_name.clone(),
            }
        } else {
            SessionListAction::None
        }
    }

    fn request_kill_selected(&mut self) {
        if let Some(session_idx) = self.selected_session_index() {
            let name = &self.sessions[session_idx].tmux_session_name;
            self.status_message = Some(format!(
                "Kill '{name}'? Press x/y to confirm, any other key to cancel."
            ));
            self.confirm_kill = Some(session_idx);
        }
    }

    fn kill_session_at(&mut self, session_idx: usize) {
        if session_idx >= self.sessions.len() {
            return;
        }
        let session = &self.sessions[session_idx];
        let name = session.tmux_session_name.clone();
        // Kill tmux session if it exists; ignore errors (session may already be gone)
        let _ = tmux::kill_session(&name);
        // Always remove from DB regardless of tmux result
        let _ = crate::session::remove_session(&name);
        self.status_message = Some(format!("Deleted session: {name}"));
        self.refresh();
    }

    pub fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();

        let form_width = 90u16.min(area.width.saturating_sub(2));
        let form_height = 30u16.min(area.height.saturating_sub(2));
        // +1 for status message below the box
        let total_height = form_height + 1;

        let vertical = Layout::vertical([Constraint::Length(total_height)])
            .flex(Flex::Center)
            .split(area);
        let horizontal = Layout::horizontal([Constraint::Length(form_width)])
            .flex(Flex::Center)
            .split(vertical[0]);
        let outer_area = horizontal[0];

        // Split into panel box and status line below
        let outer_chunks =
            Layout::vertical([Constraint::Length(form_height), Constraint::Length(1)])
                .split(outer_area);
        let panel_area = outer_chunks[0];
        let status_area = outer_chunks[1];

        frame.render_widget(Clear, panel_area);
        frame.render_widget(
            Block::default().style(Style::default().bg(theme::BG)),
            panel_area,
        );

        let outer_block = Block::default()
            .title(" Active Sessions ")
            .title_style(Style::default().fg(theme::ORANGE_BRIGHT))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::ORANGE))
            .style(Style::default().bg(theme::BG));
        let inner = outer_block.inner(panel_area);
        frame.render_widget(outer_block, panel_area);

        let chunks = Layout::vertical([
            Constraint::Min(1),    // Session list
            Constraint::Length(1), // Hints
        ])
        .split(inner);

        if self.sessions.is_empty() {
            let area = chunks[0];
            let vertical_center = area.y + area.height / 2;
            let centered_area = Rect::new(area.x, vertical_center, area.width, 1);
            let empty = Paragraph::new("No active sessions. Press 'n' to create one.")
                .style(Style::default().fg(theme::GRAY_DIM))
                .alignment(Alignment::Center);
            frame.render_widget(empty, centered_area);
        } else {
            let items: Vec<ListItem> = self
                .display_rows
                .iter()
                .map(|row| match row {
                    DisplayRow::GroupHeader(dir) => {
                        if dir.is_empty() {
                            // Blank separator line
                            ListItem::new(Line::raw(""))
                        } else {
                            let line = Line::from(vec![Span::styled(
                                dir.to_string(),
                                Style::default().fg(theme::CYAN_VIVID),
                            )]);
                            ListItem::new(line)
                        }
                    }
                    DisplayRow::Session(idx) => {
                        let s = &self.sessions[*idx];
                        let (icon, icon_color) = if s.claude_session_id.is_none() {
                            ("🖥️", theme::GRAY_DIM)
                        } else {
                            let color = match s.state {
                                SessionState::Working => theme::ORANGE_BRIGHT,
                                SessionState::WaitingUser => theme::CYAN,
                                SessionState::Idle => theme::GRAY_DIM,
                            };
                            (s.state.icon(), color)
                        };
                        let command_tag = format!("[{}]", s.claude_command);
                        // All icons display as 2 terminal columns; "▸ " highlight prefix is 2.
                        let content_used = 2 + 1 + s.tmux_session_name.len() + command_tag.len();
                        let list_width = chunks[0].width as usize;
                        let padding = list_width.saturating_sub(2 + content_used);
                        let line = Line::from(vec![
                            Span::styled(icon, Style::default().fg(icon_color)),
                            Span::raw(" "),
                            Span::styled(
                                s.tmux_session_name.clone(),
                                Style::default()
                                    .fg(theme::SESSION_NAME)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::raw(" ".repeat(padding)),
                            Span::styled(
                                command_tag,
                                Style::default().fg(theme::GRAY_DIM),
                            ),
                        ]);
                        ListItem::new(line)
                    }
                })
                .collect();

            let list = List::new(items)
                .highlight_style(
                    Style::default()
                        .bg(theme::GRAY)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol("▸ ");

            frame.render_stateful_widget(list, chunks[0], &mut self.list_state);
        }

        let hints = Paragraph::new(Line::from(Span::styled(
            "j/k: navigate  |  a: attach  |  x: kill  |  n: new task  |  t: new tmux  |  q: quit",
            Style::default().fg(theme::GRAY_DIM),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(hints, chunks[1]);

        if let Some(ref msg) = self.status_message {
            let bg = if self.confirm_kill.is_some() {
                theme::ORANGE
            } else if msg.starts_with("Killed session") {
                theme::GREEN
            } else if msg.starts_with("Kill cancelled") {
                theme::GRAY
            } else {
                theme::ERROR
            };
            let status = Paragraph::new(Line::from(Span::styled(
                format!(" {msg} "),
                Style::default().fg(Color::White).bg(bg),
            )))
            .alignment(Alignment::Center);
            frame.render_widget(status, status_area);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::SessionState;
    use crossterm::event::{KeyEventKind, KeyEventState, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    /// Three sessions, each in a different directory.
    /// BTreeMap ordering: /tmp/one, /tmp/three, /tmp/two
    /// Display rows:
    ///   [0] Header "/tmp/one"
    ///   [1] Session(0) task-one        <- first selectable
    ///   [2] Separator ""
    ///   [3] Header "/tmp/three"
    ///   [4] Session(2) task-three
    ///   [5] Separator ""
    ///   [6] Header "/tmp/two"
    ///   [7] Session(1) task-two        <- last selectable
    fn sample_sessions() -> Vec<SessionRecord> {
        vec![
            SessionRecord {
                tmux_session_id: "$1".to_string(),
                tmux_session_name: "task-one".to_string(),
                claude_session_id: None,
                directory: "/tmp/one".to_string(),
                created_at: 1000,
                state: SessionState::Idle,
                claude_command: "claude".to_string(),
            },
            SessionRecord {
                tmux_session_id: "$2".to_string(),
                tmux_session_name: "task-two".to_string(),
                claude_session_id: None,
                directory: "/tmp/two".to_string(),
                created_at: 2000,
                state: SessionState::Idle,
                claude_command: "claude".to_string(),
            },
            SessionRecord {
                tmux_session_id: "$3".to_string(),
                tmux_session_name: "task-three".to_string(),
                claude_session_id: None,
                directory: "/tmp/three".to_string(),
                created_at: 3000,
                state: SessionState::Idle,
                claude_command: "claude".to_string(),
            },
        ]
    }

    /// Two sessions share /proj/a, one in /proj/b.
    /// Display rows:
    ///   [0] Header "/proj/a"
    ///   [1] Session(0) alpha           <- first selectable
    ///   [2] Session(1) beta
    ///   [3] Separator ""
    ///   [4] Header "/proj/b"
    ///   [5] Session(2) gamma           <- last selectable
    fn sample_grouped_sessions() -> Vec<SessionRecord> {
        vec![
            SessionRecord {
                tmux_session_id: "$1".to_string(),
                tmux_session_name: "alpha".to_string(),
                claude_session_id: None,
                directory: "/proj/a".to_string(),
                created_at: 1000,
                state: SessionState::Idle,
                claude_command: "claude".to_string(),
            },
            SessionRecord {
                tmux_session_id: "$2".to_string(),
                tmux_session_name: "beta".to_string(),
                claude_session_id: None,
                directory: "/proj/a".to_string(),
                created_at: 2000,
                state: SessionState::Working,
                claude_command: "claude".to_string(),
            },
            SessionRecord {
                tmux_session_id: "$3".to_string(),
                tmux_session_name: "gamma".to_string(),
                claude_session_id: None,
                directory: "/proj/b".to_string(),
                created_at: 3000,
                state: SessionState::Idle,
                claude_command: "claude".to_string(),
            },
        ]
    }

    #[test]
    fn test_new_selects_first_session_row() {
        let list = SessionList::new(sample_sessions());
        // First selectable is display row 1 (row 0 is a header)
        assert_eq!(list.list_state.selected(), Some(1));
    }

    #[test]
    fn test_new_empty_selects_none() {
        let list = SessionList::new(vec![]);
        assert_eq!(list.list_state.selected(), None);
    }

    #[test]
    fn test_display_rows_structure() {
        let list = SessionList::new(sample_grouped_sessions());
        // Expected: Header, Session, Session, Separator, Header, Session
        assert_eq!(list.display_rows.len(), 6);
        assert!(matches!(list.display_rows[0], DisplayRow::GroupHeader(ref d) if d == "/proj/a"));
        assert!(matches!(list.display_rows[1], DisplayRow::Session(0)));
        assert!(matches!(list.display_rows[2], DisplayRow::Session(1)));
        assert!(matches!(list.display_rows[3], DisplayRow::GroupHeader(ref d) if d.is_empty()));
        assert!(matches!(list.display_rows[4], DisplayRow::GroupHeader(ref d) if d == "/proj/b"));
        assert!(matches!(list.display_rows[5], DisplayRow::Session(2)));
    }

    #[test]
    fn test_move_down_skips_headers() {
        let mut list = SessionList::new(sample_grouped_sessions());
        // Starts at row 1 (alpha)
        assert_eq!(list.list_state.selected(), Some(1));

        list.handle_key(key(KeyCode::Char('j')));
        // Should go to row 2 (beta), not row 3 (separator)
        assert_eq!(list.list_state.selected(), Some(2));

        list.handle_key(key(KeyCode::Char('j')));
        // Should skip separator + header, land on row 5 (gamma)
        assert_eq!(list.list_state.selected(), Some(5));
    }

    #[test]
    fn test_move_down_wraps() {
        let mut list = SessionList::new(sample_grouped_sessions());
        // Go to last session (gamma, row 5)
        list.list_state.select(Some(5));
        list.handle_key(key(KeyCode::Down));
        // Should wrap to first selectable (alpha, row 1)
        assert_eq!(list.list_state.selected(), Some(1));
    }

    #[test]
    fn test_move_up_skips_headers() {
        let mut list = SessionList::new(sample_grouped_sessions());
        // Start at gamma (row 5)
        list.list_state.select(Some(5));

        list.handle_key(key(KeyCode::Char('k')));
        // Should skip separator + header, land on beta (row 2)
        assert_eq!(list.list_state.selected(), Some(2));
    }

    #[test]
    fn test_move_up_wraps() {
        let mut list = SessionList::new(sample_grouped_sessions());
        // At first selectable (alpha, row 1)
        list.handle_key(key(KeyCode::Up));
        // Should wrap to last selectable (gamma, row 5)
        assert_eq!(list.list_state.selected(), Some(5));
    }

    #[test]
    fn test_q_quits() {
        let mut list = SessionList::new(sample_sessions());
        let action = list.handle_key(key(KeyCode::Char('q')));
        assert_eq!(action, SessionListAction::Quit);
    }

    #[test]
    fn test_esc_does_not_quit() {
        let mut list = SessionList::new(sample_sessions());
        let action = list.handle_key(key(KeyCode::Esc));
        assert_eq!(action, SessionListAction::None);
    }

    #[test]
    fn test_n_new_task() {
        let mut list = SessionList::new(sample_sessions());
        let action = list.handle_key(key(KeyCode::Char('n')));
        assert_eq!(action, SessionListAction::NewTask);
    }

    #[test]
    fn test_a_attaches_selected() {
        let mut list = SessionList::new(sample_grouped_sessions());
        // Starts at alpha (row 1). Move down to beta (row 2).
        list.handle_key(key(KeyCode::Down));
        let action = list.handle_key(key(KeyCode::Char('a')));
        assert_eq!(
            action,
            SessionListAction::Attach {
                session_name: "beta".to_string()
            }
        );
    }

    #[test]
    fn test_enter_attaches_selected() {
        let mut list = SessionList::new(sample_grouped_sessions());
        list.handle_key(key(KeyCode::Down)); // beta
        let action = list.handle_key(key(KeyCode::Enter));
        assert_eq!(
            action,
            SessionListAction::Attach {
                session_name: "beta".to_string()
            }
        );
    }

    #[test]
    fn test_a_on_empty_is_noop() {
        let mut list = SessionList::new(vec![]);
        let action = list.handle_key(key(KeyCode::Char('a')));
        assert_eq!(action, SessionListAction::None);
    }

    #[test]
    fn test_navigation_on_empty_is_noop() {
        let mut list = SessionList::new(vec![]);
        list.handle_key(key(KeyCode::Down));
        assert_eq!(list.list_state.selected(), None);
        list.handle_key(key(KeyCode::Up));
        assert_eq!(list.list_state.selected(), None);
    }

    #[test]
    fn test_select_by_name_finds_session() {
        let mut list = SessionList::new(sample_grouped_sessions());
        list.select_by_name("gamma");
        // gamma is session index 2, display row 5
        assert_eq!(list.list_state.selected(), Some(5));
    }

    #[test]
    fn test_select_by_name_falls_back_to_first() {
        let mut list = SessionList::new(sample_grouped_sessions());
        list.select_by_name("nonexistent");
        // Falls back to first selectable (row 1)
        assert_eq!(list.list_state.selected(), Some(1));
    }

    #[test]
    fn test_clamp_selection_selects_first_when_none() {
        let mut list = SessionList::new(sample_sessions());
        list.list_state.select(None);
        list.clamp_selection();
        // First selectable row
        assert_eq!(list.list_state.selected(), Some(1));
    }

    #[test]
    fn test_x_sets_confirm_kill() {
        let mut list = SessionList::new(sample_grouped_sessions());
        // Selected is alpha (session index 0)
        let action = list.handle_key(key(KeyCode::Char('x')));
        assert_eq!(action, SessionListAction::None);
        assert_eq!(list.confirm_kill, Some(0));
        assert!(list.status_message.as_ref().unwrap().contains("confirm"));
    }

    #[test]
    fn test_t_new_tmux_session() {
        let mut list = SessionList::new(sample_sessions());
        let action = list.handle_key(key(KeyCode::Char('t')));
        assert_eq!(action, SessionListAction::NewTmuxSession);
    }

    #[test]
    fn test_x_on_empty_is_noop() {
        let mut list = SessionList::new(vec![]);
        list.handle_key(key(KeyCode::Char('x')));
        assert_eq!(list.confirm_kill, None);
    }

    #[test]
    fn test_confirm_kill_cancelled_by_other_key() {
        let mut list = SessionList::new(sample_sessions());
        list.handle_key(key(KeyCode::Char('x'))); // enter confirmation
        assert!(list.confirm_kill.is_some());

        list.handle_key(key(KeyCode::Esc)); // cancel
        assert_eq!(list.confirm_kill, None);
        assert_eq!(list.status_message.as_ref().unwrap(), "Kill cancelled.");
    }

    #[test]
    fn test_confirm_kill_cancelled_preserves_sessions() {
        let mut list = SessionList::new(sample_sessions());
        let count_before = list.sessions.len();
        list.handle_key(key(KeyCode::Char('x'))); // enter confirmation
        list.handle_key(key(KeyCode::Char('n'))); // cancel with 'n'
        assert_eq!(list.sessions.len(), count_before);
        assert_eq!(list.confirm_kill, None);
    }

    #[test]
    fn test_navigation_during_confirm_cancels() {
        let mut list = SessionList::new(sample_sessions());
        list.handle_key(key(KeyCode::Char('x'))); // enter confirmation
        assert!(list.confirm_kill.is_some());

        list.handle_key(key(KeyCode::Char('j'))); // navigation key cancels
        assert_eq!(list.confirm_kill, None);
        assert_eq!(list.status_message.as_ref().unwrap(), "Kill cancelled.");
    }

    #[test]
    fn test_single_group_no_separator() {
        // All sessions in same directory — no separator rows
        let sessions = vec![
            SessionRecord {
                tmux_session_id: "$1".to_string(),
                tmux_session_name: "a".to_string(),
                claude_session_id: None,
                directory: "/same".to_string(),
                created_at: 1000,
                state: SessionState::Idle,
                claude_command: "claude".to_string(),
            },
            SessionRecord {
                tmux_session_id: "$2".to_string(),
                tmux_session_name: "b".to_string(),
                claude_session_id: None,
                directory: "/same".to_string(),
                created_at: 2000,
                state: SessionState::Idle,
                claude_command: "claude".to_string(),
            },
        ];
        let list = SessionList::new(sessions);
        // Header + 2 sessions, no separators
        assert_eq!(list.display_rows.len(), 3);
        assert!(matches!(list.display_rows[0], DisplayRow::GroupHeader(ref d) if d == "/same"));
        assert!(matches!(list.display_rows[1], DisplayRow::Session(0)));
        assert!(matches!(list.display_rows[2], DisplayRow::Session(1)));
    }

    #[test]
    fn test_attach_correct_session_across_groups() {
        let mut list = SessionList::new(sample_grouped_sessions());
        // Navigate to gamma (last session, row 5)
        list.handle_key(key(KeyCode::Down)); // beta (row 2)
        list.handle_key(key(KeyCode::Down)); // gamma (row 5)
        let action = list.handle_key(key(KeyCode::Char('a')));
        assert_eq!(
            action,
            SessionListAction::Attach {
                session_name: "gamma".to_string()
            }
        );
    }
}
