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

#[derive(Debug)]
pub struct SessionList {
    pub sessions: Vec<SessionRecord>,
    pub list_state: ListState,
    pub status_message: Option<String>,
    /// When set, we're waiting for the user to confirm killing the session at this index.
    confirm_kill: Option<usize>,
}

impl SessionList {
    pub fn new(sessions: Vec<SessionRecord>) -> Self {
        let mut list_state = ListState::default();
        if !sessions.is_empty() {
            list_state.select(Some(0));
        }
        Self {
            sessions,
            list_state,
            status_message: None,
            confirm_kill: None,
        }
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

    /// Select a session by tmux name. Falls back to first item if not found.
    pub fn select_by_name(&mut self, name: &str) {
        let idx = self
            .sessions
            .iter()
            .position(|s| s.tmux_session_name == name)
            .unwrap_or(0);
        if !self.sessions.is_empty() {
            self.list_state.select(Some(idx));
        }
    }

    fn clamp_selection(&mut self) {
        if self.sessions.is_empty() {
            self.list_state.select(None);
        } else if self.list_state.selected().is_none() {
            self.list_state.select(Some(0));
        } else if let Some(i) = self.list_state.selected()
            && i >= self.sessions.len()
        {
            self.list_state.select(Some(self.sessions.len() - 1));
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
        if self.sessions.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.sessions.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    fn move_down(&mut self) {
        if self.sessions.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => {
                if i >= self.sessions.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    fn attach_selected(&self) -> SessionListAction {
        if let Some(i) = self.list_state.selected() {
            let session = &self.sessions[i];
            SessionListAction::Attach {
                session_name: session.tmux_session_name.clone(),
            }
        } else {
            SessionListAction::None
        }
    }

    fn request_kill_selected(&mut self) {
        if let Some(i) = self.list_state.selected() {
            let name = &self.sessions[i].tmux_session_name;
            self.status_message = Some(format!(
                "Kill '{name}'? Press x/y to confirm, any other key to cancel."
            ));
            self.confirm_kill = Some(i);
        }
    }

    fn kill_session_at(&mut self, idx: usize) {
        if idx >= self.sessions.len() {
            return;
        }
        let session = &self.sessions[idx];
        let name = session.tmux_session_name.clone();
        match tmux::kill_session(&name) {
            Ok(()) => {
                self.status_message = Some(format!("Killed session: {name}"));
                // Remove from our DB too
                let _ = crate::session::remove_session(&name);
                self.refresh();
            }
            Err(e) => {
                self.status_message = Some(format!("Failed to kill '{name}': {e}"));
            }
        }
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
                .sessions
                .iter()
                .map(|s| {
                    let icon_color = match s.state {
                        SessionState::Working => theme::ORANGE_BRIGHT,
                        SessionState::WaitingUser => Color::Yellow,
                        SessionState::Idle => theme::GRAY_DIM,
                    };
                    let line = Line::from(vec![
                        Span::styled(s.state.icon(), Style::default().fg(icon_color)),
                        Span::raw(" "),
                        Span::styled(
                            &s.tmux_session_name,
                            Style::default()
                                .fg(theme::SESSION_NAME)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::raw("  "),
                        Span::styled(&s.directory, Style::default().fg(theme::GRAY_DIM)),
                    ]);
                    ListItem::new(line)
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
            } else if msg.starts_with("Kill cancelled") || msg.starts_with("Killed session") {
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

    fn sample_sessions() -> Vec<SessionRecord> {
        vec![
            SessionRecord {
                tmux_session_id: "$1".to_string(),
                tmux_session_name: "task-one".to_string(),
                claude_session_id: None,
                directory: "/tmp/one".to_string(),
                created_at: 1000,
                state: SessionState::Idle,
            },
            SessionRecord {
                tmux_session_id: "$2".to_string(),
                tmux_session_name: "task-two".to_string(),
                claude_session_id: None,
                directory: "/tmp/two".to_string(),
                created_at: 2000,
                state: SessionState::Idle,
            },
            SessionRecord {
                tmux_session_id: "$3".to_string(),
                tmux_session_name: "task-three".to_string(),
                claude_session_id: None,
                directory: "/tmp/three".to_string(),
                created_at: 3000,
                state: SessionState::Idle,
            },
        ]
    }

    #[test]
    fn test_new_selects_first() {
        let list = SessionList::new(sample_sessions());
        assert_eq!(list.list_state.selected(), Some(0));
    }

    #[test]
    fn test_new_empty_selects_none() {
        let list = SessionList::new(vec![]);
        assert_eq!(list.list_state.selected(), None);
    }

    #[test]
    fn test_move_down() {
        let mut list = SessionList::new(sample_sessions());
        list.handle_key(key(KeyCode::Char('j')));
        assert_eq!(list.list_state.selected(), Some(1));
    }

    #[test]
    fn test_move_down_wraps() {
        let mut list = SessionList::new(sample_sessions());
        list.list_state.select(Some(2));
        list.handle_key(key(KeyCode::Down));
        assert_eq!(list.list_state.selected(), Some(0));
    }

    #[test]
    fn test_move_up() {
        let mut list = SessionList::new(sample_sessions());
        list.list_state.select(Some(2));
        list.handle_key(key(KeyCode::Char('k')));
        assert_eq!(list.list_state.selected(), Some(1));
    }

    #[test]
    fn test_move_up_wraps() {
        let mut list = SessionList::new(sample_sessions());
        list.handle_key(key(KeyCode::Up));
        assert_eq!(list.list_state.selected(), Some(2));
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
        let mut list = SessionList::new(sample_sessions());
        list.handle_key(key(KeyCode::Down)); // select task-two
        let action = list.handle_key(key(KeyCode::Char('a')));
        assert_eq!(
            action,
            SessionListAction::Attach {
                session_name: "task-two".to_string()
            }
        );
    }

    #[test]
    fn test_enter_attaches_selected() {
        let mut list = SessionList::new(sample_sessions());
        list.handle_key(key(KeyCode::Down)); // select task-two
        let action = list.handle_key(key(KeyCode::Enter));
        assert_eq!(
            action,
            SessionListAction::Attach {
                session_name: "task-two".to_string()
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
        let mut list = SessionList::new(sample_sessions());
        list.select_by_name("task-three");
        assert_eq!(list.list_state.selected(), Some(2));
    }

    #[test]
    fn test_select_by_name_falls_back_to_first() {
        let mut list = SessionList::new(sample_sessions());
        list.select_by_name("nonexistent");
        assert_eq!(list.list_state.selected(), Some(0));
    }

    #[test]
    fn test_clamp_selection_selects_first_when_none() {
        let mut list = SessionList::new(sample_sessions());
        list.list_state.select(None);
        list.clamp_selection();
        assert_eq!(list.list_state.selected(), Some(0));
    }

    #[test]
    fn test_x_sets_confirm_kill() {
        let mut list = SessionList::new(sample_sessions());
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
}
