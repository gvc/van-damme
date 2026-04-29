use std::collections::BTreeMap;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, ListState, Paragraph},
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
    /// Non-selectable directory group header. String value used in tests.
    #[allow(dead_code)]
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
    /// Top card index for the scrollable card list.
    card_scroll_offset: usize,
    /// Cached tmux pane content for the selected session.
    preview_content: Option<String>,
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
            card_scroll_offset: 0,
            preview_content: None,
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

    /// Refresh the cached tmux pane preview for the currently selected session.
    /// Call this on tick rather than on every render to avoid subprocess overhead.
    pub fn refresh_preview(&mut self) {
        if let Some(idx) = self.selected_session_index() {
            let session_name = self.sessions[idx].tmux_session_name.clone();
            self.preview_content = tmux::capture_pane(&session_name).ok();
        } else {
            self.preview_content = None;
        }
    }

    pub fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();

        frame.render_widget(
            Block::default().style(Style::default().bg(theme::BG)),
            area,
        );

        const LEFT_WIDTH: u16 = 54;
        let chunks = Layout::horizontal([
            Constraint::Length(LEFT_WIDTH.min(area.width)),
            Constraint::Min(0),
        ])
        .split(area);

        self.draw_session_panel(frame, chunks[0]);
        self.draw_preview_panel(frame, chunks[1]);

        // Status message overlay at bottom center
        if let Some(ref msg) = self.status_message {
            let bg = if self.confirm_kill.is_some() {
                theme::ORANGE
            } else if msg.starts_with("Deleted session") {
                theme::GREEN
            } else if msg.starts_with("Kill cancelled") {
                theme::GRAY
            } else {
                theme::ERROR
            };
            let msg_text = format!(" {msg} ");
            let status_width = (msg_text.len() as u16).min(area.width);
            let status_x = area.x + area.width.saturating_sub(status_width) / 2;
            let status_y = area.y + area.height.saturating_sub(1);
            let status_area = Rect::new(status_x, status_y, status_width, 1);
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    msg_text,
                    Style::default().fg(Color::White).bg(bg),
                )))
                .alignment(Alignment::Center),
                status_area,
            );
        }
    }

    fn draw_session_panel(&mut self, frame: &mut Frame, area: Rect) {
        let selected_session_idx = self.selected_session_index();

        let outer_block = Block::default()
            .title(" ACTIVE SESSIONS ")
            .title_style(
                Style::default()
                    .fg(theme::ORANGE_BRIGHT)
                    .add_modifier(Modifier::BOLD),
            )
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::ORANGE))
            .style(Style::default().bg(theme::BG));
        let inner = outer_block.inner(area);
        frame.render_widget(outer_block, area);

        let chunks = Layout::vertical([
            Constraint::Min(0),
            Constraint::Length(2), // hints
        ])
        .split(inner);
        let cards_area = chunks[0];
        let hints_area = chunks[1];

        if self.sessions.is_empty() {
            let y = cards_area.y + cards_area.height.saturating_sub(2) / 2;
            frame.render_widget(
                Paragraph::new("No active sessions.")
                    .style(Style::default().fg(theme::GRAY_DIM))
                    .alignment(Alignment::Center),
                Rect::new(cards_area.x, y, cards_area.width, 1),
            );
            if y + 1 < cards_area.y + cards_area.height {
                frame.render_widget(
                    Paragraph::new("Press 'n' to create one.")
                        .style(Style::default().fg(theme::GRAY_DIM))
                        .alignment(Alignment::Center),
                    Rect::new(cards_area.x, y + 1, cards_area.width, 1),
                );
            }
        } else {
            const CARD_HEIGHT: u16 = 5;
            let visible_count = (cards_area.height / CARD_HEIGHT) as usize;

            // Auto-scroll to keep selected card visible.
            if let Some(sel_idx) = selected_session_idx {
                if sel_idx < self.card_scroll_offset {
                    self.card_scroll_offset = sel_idx;
                } else if visible_count > 0 && sel_idx >= self.card_scroll_offset + visible_count {
                    self.card_scroll_offset = sel_idx.saturating_sub(visible_count - 1);
                }
            }

            let scroll = self.card_scroll_offset;
            let mut y = cards_area.y;
            for (i, session) in self.sessions.iter().enumerate().skip(scroll) {
                if y + CARD_HEIGHT > cards_area.y + cards_area.height {
                    break;
                }
                let card_area = Rect::new(cards_area.x, y, cards_area.width, CARD_HEIGHT);
                draw_session_card(frame, card_area, session, selected_session_idx == Some(i));
                y += CARD_HEIGHT;
            }
        }

        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    "j/k:navigate · a:attach · x:kill",
                    Style::default().fg(theme::GRAY_DIM),
                )),
                Line::from(Span::styled(
                    "n:new task · t:tmux · q:quit",
                    Style::default().fg(theme::GRAY_DIM),
                )),
            ])
            .alignment(Alignment::Center),
            hints_area,
        );
    }

    fn draw_preview_panel(&self, frame: &mut Frame, area: Rect) {
        if area.width < 4 {
            return;
        }

        let selected = self.selected_session_index().map(|i| &self.sessions[i]);

        let (title, border_fg) = match selected {
            Some(s) => (format!(" {} ", s.tmux_session_name), theme::ORANGE),
            None => (" no session selected ".to_string(), theme::BLUE),
        };

        let outer_block = Block::default()
            .title(title.as_str())
            .title_style(Style::default().fg(theme::ORANGE_BRIGHT))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_fg))
            .style(Style::default().bg(theme::BG));
        let inner = outer_block.inner(area);
        frame.render_widget(outer_block, area);

        if let Some(ref content) = self.preview_content {
            let lines: Vec<Line> = content
                .lines()
                .map(|l| Line::raw(l.trim_end().to_string()))
                .collect();
            let visible_height = inner.height as usize;
            let skip = lines.len().saturating_sub(visible_height);
            let visible: Vec<Line> = lines.into_iter().skip(skip).collect();
            frame.render_widget(
                Paragraph::new(visible).style(Style::default().fg(theme::TEXT)),
                inner,
            );
        } else {
            draw_ascii_art(frame, inner);
        }
    }
}

fn draw_session_card(
    frame: &mut Frame,
    area: Rect,
    session: &SessionRecord,
    is_selected: bool,
) {
    let (icon, state_color) = if session.claude_session_id.is_none() {
        // Plain tmux session — use a neutral indicator
        ("🖥", theme::GRAY_DIM)
    } else {
        let color = match session.state {
            SessionState::Working => theme::ORANGE_BRIGHT,
            SessionState::WaitingUser => theme::CYAN,
            SessionState::Idle => theme::GRAY_DIM,
        };
        (session.state.icon(), color)
    };

    let border_fg = if is_selected {
        theme::ORANGE_BRIGHT
    } else {
        theme::BLUE
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_fg))
        .style(Style::default().bg(theme::BG));
    let card_inner = block.inner(area);
    frame.render_widget(block, area);

    if card_inner.height < 3 || card_inner.width < 5 {
        return;
    }

    let content_w = card_inner.width as usize;
    // All state icons are emoji and display as 2 terminal columns.
    const ICON_COLS: usize = 2;
    const ICON_GAP: usize = 1;
    const PREFIX: usize = ICON_COLS + ICON_GAP; // 3 cols before name text

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(card_inner);

    // Row 0: icon + session name
    let max_name = content_w.saturating_sub(PREFIX);
    let name = truncate_str(&session.tmux_session_name, max_name);
    let name_style = if is_selected {
        Style::default()
            .fg(theme::ORANGE_BRIGHT)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(theme::SESSION_NAME)
            .add_modifier(Modifier::BOLD)
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(icon, Style::default().fg(state_color)),
            Span::raw(" "),
            Span::styled(name, name_style),
        ])),
        rows[0],
    );

    // Row 1: directory (indented to align with name)
    let dir = shorten_path(&session.directory, content_w.saturating_sub(PREFIX));
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw("   "),
            Span::styled(dir, Style::default().fg(theme::GRAY_DIM)),
        ])),
        rows[1],
    );

    // Row 2: command · model
    let cmd_tag = match &session.model_id {
        Some(m) => format!("{} · {}", session.claude_command, shorten_model_id(m)),
        None => session.claude_command.clone(),
    };
    let cmd_tag = truncate_str(&cmd_tag, content_w.saturating_sub(PREFIX));
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw("   "),
            Span::styled(cmd_tag, Style::default().fg(theme::GRAY_DIM)),
        ])),
        rows[2],
    );
}

fn draw_ascii_art(frame: &mut Frame, area: Rect) {
    if area.height < 3 || area.width < 10 {
        return;
    }

    const VAN_ART: &[&str] = &[
        " ██╗   ██╗ █████╗ ███╗   ██╗",
        " ██║   ██║██╔══██╗████╗  ██║",
        " ██║   ██║███████║██╔██╗ ██║",
        " ╚██╗ ██╔╝██╔══██║██║╚██╗██║",
        "  ╚████╔╝ ██║  ██║██║ ╚████║",
        "   ╚═══╝  ╚═╝  ╚═╝╚═╝  ╚═══╝",
    ];
    const DAMME_ART: &[&str] = &[
        " ██████╗  █████╗ ███╗   ███╗███╗   ███╗███████╗",
        " ██╔══██╗██╔══██╗████╗ ████║████╗ ████║██╔════╝",
        " ██║  ██║███████║██╔████╔██║██╔████╔██║█████╗  ",
        " ██║  ██║██╔══██║██║╚██╔╝██║██║╚██╔╝██║██╔══╝  ",
        " ██████╔╝██║  ██║██║ ╚═╝ ██║██║ ╚═╝ ██║███████╗",
        " ╚═════╝ ╚═╝  ╚═╝╚═╝     ╚═╝╚═╝     ╚═╝╚══════╝",
    ];
    const TAGLINE: &str = "tmux × claude session manager";

    // VAN(6) + gap(1) + DAMME(6) + gap(1) + tagline(1) = 15 rows
    let total_h = (VAN_ART.len() + 1 + DAMME_ART.len() + 2) as u16;
    let start_y = area.y + area.height.saturating_sub(total_h) / 2;
    let mut y = start_y;

    for line in VAN_ART {
        if y >= area.y + area.height {
            break;
        }
        frame.render_widget(
            Paragraph::new(Span::styled(*line, Style::default().fg(theme::ORANGE_BRIGHT)))
                .alignment(Alignment::Center),
            Rect::new(area.x, y, area.width, 1),
        );
        y += 1;
    }

    y += 1; // gap between VAN and DAMME

    for line in DAMME_ART {
        if y >= area.y + area.height {
            break;
        }
        frame.render_widget(
            Paragraph::new(Span::styled(*line, Style::default().fg(theme::ORANGE)))
                .alignment(Alignment::Center),
            Rect::new(area.x, y, area.width, 1),
        );
        y += 1;
    }

    y += 2; // gap before tagline

    if y < area.y + area.height {
        frame.render_widget(
            Paragraph::new(Span::styled(TAGLINE, Style::default().fg(theme::GRAY_DIM)))
                .alignment(Alignment::Center),
            Rect::new(area.x, y, area.width, 1),
        );
    }
}

/// Replace the home directory prefix with `~`.  Truncates with `…` if still too long.
fn shorten_path(path: &str, max_len: usize) -> String {
    if max_len == 0 {
        return String::new();
    }
    let tilde = if let Some(home) = dirs::home_dir() {
        let h = home.to_string_lossy();
        if path.starts_with(h.as_ref()) {
            format!("~{}", &path[h.len()..])
        } else {
            path.to_string()
        }
    } else {
        path.to_string()
    };
    truncate_str(&tilde, max_len)
}

/// Truncate `s` to at most `max_len` Unicode codepoints, appending `…` if cut.
fn truncate_str(s: &str, max_len: usize) -> String {
    if max_len == 0 {
        return String::new();
    }
    if s.chars().count() <= max_len {
        return s.to_string();
    }
    let t: String = s.chars().take(max_len.saturating_sub(1)).collect();
    format!("{t}…")
}

/// Strip the `claude-` prefix from a model ID for compact display.
fn shorten_model_id(model_id: &str) -> &str {
    model_id.strip_prefix("claude-").unwrap_or(model_id)
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
                model_id: None,
            },
            SessionRecord {
                tmux_session_id: "$2".to_string(),
                tmux_session_name: "task-two".to_string(),
                claude_session_id: None,
                directory: "/tmp/two".to_string(),
                created_at: 2000,
                state: SessionState::Idle,
                claude_command: "claude".to_string(),
                model_id: None,
            },
            SessionRecord {
                tmux_session_id: "$3".to_string(),
                tmux_session_name: "task-three".to_string(),
                claude_session_id: None,
                directory: "/tmp/three".to_string(),
                created_at: 3000,
                state: SessionState::Idle,
                claude_command: "claude".to_string(),
                model_id: None,
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
                model_id: None,
            },
            SessionRecord {
                tmux_session_id: "$2".to_string(),
                tmux_session_name: "beta".to_string(),
                claude_session_id: None,
                directory: "/proj/a".to_string(),
                created_at: 2000,
                state: SessionState::Working,
                claude_command: "claude".to_string(),
                model_id: None,
            },
            SessionRecord {
                tmux_session_id: "$3".to_string(),
                tmux_session_name: "gamma".to_string(),
                claude_session_id: None,
                directory: "/proj/b".to_string(),
                created_at: 3000,
                state: SessionState::Idle,
                claude_command: "claude".to_string(),
                model_id: None,
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
                model_id: None,
            },
            SessionRecord {
                tmux_session_id: "$2".to_string(),
                tmux_session_name: "b".to_string(),
                claude_session_id: None,
                directory: "/same".to_string(),
                created_at: 2000,
                state: SessionState::Idle,
                claude_command: "claude".to_string(),
                model_id: None,
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

    #[test]
    fn test_command_tag_without_model() {
        let session = SessionRecord {
            tmux_session_id: "$1".to_string(),
            tmux_session_name: "test".to_string(),
            claude_session_id: None,
            directory: "/tmp".to_string(),
            created_at: 1000,
            state: SessionState::Idle,
            claude_command: "claude".to_string(),
            model_id: None,
        };
        let tag = match &session.model_id {
            Some(m) => format!("[{} | {}]", session.claude_command, m),
            None => format!("[{}]", session.claude_command),
        };
        assert_eq!(tag, "[claude]");
    }

    #[test]
    fn test_command_tag_with_model() {
        let session = SessionRecord {
            tmux_session_id: "$1".to_string(),
            tmux_session_name: "test".to_string(),
            claude_session_id: None,
            directory: "/tmp".to_string(),
            created_at: 1000,
            state: SessionState::Idle,
            claude_command: "claude".to_string(),
            model_id: Some("claude-sonnet-4-6".to_string()),
        };
        let tag = match &session.model_id {
            Some(m) => format!("[{} | {}]", session.claude_command, m),
            None => format!("[{}]", session.claude_command),
        };
        assert_eq!(tag, "[claude | claude-sonnet-4-6]");
    }
}
