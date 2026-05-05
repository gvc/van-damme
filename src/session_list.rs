use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::grouped_list::{GroupedList, VisibleRow};
use crate::session::{SessionRecord, SessionState};
use crate::theme;
use crate::tmux;

fn group_key(s: &SessionRecord) -> &str {
    &s.directory
}

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
    list: GroupedList<SessionRecord>,
    pub status_message: Option<String>,
    confirm_kill: Option<String>,
    card_scroll_offset: usize,
    preview_content: Option<String>,
    preview_summary: Option<tmux::SessionSummary>,
}

impl SessionList {
    pub fn new(sessions: Vec<SessionRecord>) -> Self {
        Self {
            list: GroupedList::new(sessions, group_key),
            status_message: None,
            confirm_kill: None,
            card_scroll_offset: 0,
            preview_content: None,
            preview_summary: None,
        }
    }

    #[allow(dead_code)]
    pub fn sessions(&self) -> &[SessionRecord] {
        self.list.items()
    }

    pub fn refresh(&mut self) {
        let sessions = crate::session::default_db_path()
            .and_then(|p| crate::session::SessionDb::open(&p))
            .map(|db| db.sessions.clone());
        match sessions {
            Ok(sessions) => {
                let alive: Vec<SessionRecord> = sessions
                    .into_iter()
                    .filter(|s| tmux::session_exists(&s.tmux_session_name).unwrap_or(false))
                    .collect();
                self.list.replace_items(alive, group_key);
            }
            Err(e) => {
                self.status_message = Some(format!("Error loading sessions: {e}"));
            }
        }
    }

    pub fn refresh_states(&mut self) {
        let Ok(db_sessions) = crate::session::default_db_path()
            .and_then(|p| crate::session::SessionDb::open(&p))
            .map(|db| db.sessions.clone())
        else {
            return;
        };
        for session in self.list.items_mut() {
            if let Some(updated) = db_sessions
                .iter()
                .find(|s: &&SessionRecord| s.tmux_session_name == session.tmux_session_name)
            {
                session.state = updated.state.clone();
            }
        }
    }

    pub fn select_by_name(&mut self, name: &str) {
        self.list.select_by(|s| s.tmux_session_name == name);
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> SessionListAction {
        if let Some(ref name) = self.confirm_kill.clone() {
            self.confirm_kill = None;
            match key.code {
                KeyCode::Char('x') | KeyCode::Char('y') => {
                    self.kill_session(name);
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
                self.list.move_up();
                SessionListAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.list.move_down();
                SessionListAction::None
            }
            KeyCode::Enter | KeyCode::Char('a') => self.attach_selected(),
            KeyCode::Char('x') => {
                self.request_kill_selected();
                SessionListAction::None
            }
            KeyCode::Char('z') => {
                self.list.toggle_collapse_selected(group_key);
                SessionListAction::None
            }
            KeyCode::Char('Z') => {
                self.list.toggle_collapse_all(group_key);
                SessionListAction::None
            }
            _ => SessionListAction::None,
        }
    }

    fn attach_selected(&self) -> SessionListAction {
        if let Some(session) = self.list.selected_item() {
            SessionListAction::Attach {
                session_name: session.tmux_session_name.clone(),
            }
        } else {
            SessionListAction::None
        }
    }

    fn request_kill_selected(&mut self) {
        if let Some(session) = self.list.selected_item() {
            let name = session.tmux_session_name.clone();
            self.status_message = Some(format!(
                "Kill '{name}'? Press x/y to confirm, any other key to cancel."
            ));
            self.confirm_kill = Some(name);
        }
    }

    fn kill_session(&mut self, name: &str) {
        let _ = tmux::kill_session(name);
        if let Ok(path) = crate::session::default_db_path()
            && let Ok(mut db) = crate::session::SessionDb::open(&path)
        {
            db.sessions.retain(|s| s.tmux_session_name != name);
            let _ = db.save();
        }
        self.status_message = Some(format!("Deleted session: {name}"));
        self.refresh();
    }

    pub fn refresh_preview(&mut self) {
        if let Some(session) = self.list.selected_item() {
            let session_name = session.tmux_session_name.clone();
            self.preview_content = tmux::capture_pane(&session_name).ok();
            self.preview_summary = tmux::session_summary(&session_name).ok();
        } else {
            self.preview_content = None;
            self.preview_summary = None;
        }
    }

    pub fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();

        frame.render_widget(Block::default().style(Style::default().bg(theme::BG)), area);

        const LEFT_WIDTH: u16 = 54;
        let chunks = Layout::horizontal([
            Constraint::Length(LEFT_WIDTH.min(area.width)),
            Constraint::Min(0),
        ])
        .split(area);

        self.draw_session_panel(frame, chunks[0]);
        self.draw_preview_panel(frame, chunks[1]);

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

        let chunks = Layout::vertical([Constraint::Min(0), Constraint::Length(2)]).split(inner);
        let cards_area = chunks[0];
        let hints_area = chunks[1];

        if self.list.is_empty() {
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

            // Compute row heights and total for scroll
            let row_heights: Vec<u16> = self
                .list
                .visible_rows()
                .map(|r| match r {
                    VisibleRow::Item { .. } => CARD_HEIGHT,
                    _ => 1,
                })
                .collect();

            // Auto-scroll: keep selected row visible
            if let Some(sel_display) = self.list.selected_display_index() {
                let sel_y: u16 = row_heights[..sel_display].iter().sum();
                let sel_h = row_heights[sel_display];
                let scroll_y: u16 = row_heights[..self.card_scroll_offset].iter().sum();
                let visible_height = cards_area.height;

                if sel_y < scroll_y {
                    self.card_scroll_offset = sel_display;
                } else if sel_y + sel_h > scroll_y + visible_height {
                    let mut offset = self.card_scroll_offset;
                    loop {
                        let top: u16 = row_heights[..offset].iter().sum();
                        let bottom = sel_y + sel_h - top;
                        if bottom <= visible_height || offset >= sel_display {
                            break;
                        }
                        offset += 1;
                    }
                    self.card_scroll_offset = offset;
                }
            }

            let mut y = cards_area.y;
            for (row_idx, row) in self.list.visible_rows().enumerate() {
                if row_idx < self.card_scroll_offset {
                    continue;
                }
                if y >= cards_area.y + cards_area.height {
                    break;
                }

                let is_selected = self.list.is_selected_row(row_idx);

                match row {
                    VisibleRow::Separator => {
                        y += 1;
                    }
                    VisibleRow::GroupHeader { dir, collapsed } => {
                        let arrow = if collapsed { "▶ " } else { "▼ " };
                        let short_dir =
                            shorten_path(dir, (cards_area.width as usize).saturating_sub(3));
                        let header_text = format!("{arrow}{short_dir}");
                        let header_style = if is_selected {
                            Style::default()
                                .fg(theme::ORANGE_BRIGHT)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default()
                                .fg(theme::CYAN)
                                .add_modifier(Modifier::BOLD)
                        };
                        frame.render_widget(
                            Paragraph::new(Line::from(vec![Span::styled(
                                header_text,
                                header_style,
                            )])),
                            Rect::new(cards_area.x + 1, y, cards_area.width.saturating_sub(2), 1),
                        );
                        y += 1;
                    }
                    VisibleRow::Item { item, selected } => {
                        if y + CARD_HEIGHT > cards_area.y + cards_area.height {
                            break;
                        }
                        let card_area = Rect::new(cards_area.x, y, cards_area.width, CARD_HEIGHT);
                        draw_session_card(frame, card_area, item, selected);
                        y += CARD_HEIGHT;
                    }
                }
            }
        }

        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    "j/k:navigate · a:attach · x:kill · z:collapse · Z:all",
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

        let selected = self.list.selected_item();

        let (title, border_fg) = match selected {
            Some(s) => (format!(" {} ", s.tmux_session_name), theme::ORANGE),
            None => match self.list.selected_header() {
                Some(dir) => (format!(" {} ", shorten_path(dir, 40)), theme::CYAN),
                None => (" no session selected ".to_string(), theme::BLUE),
            },
        };

        let outer_block = Block::default()
            .title(title.as_str())
            .title_style(Style::default().fg(theme::ORANGE_BRIGHT))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_fg))
            .style(Style::default().bg(theme::BG));
        let inner = outer_block.inner(area);
        frame.render_widget(outer_block, area);

        let has_summary = self.preview_summary.is_some();
        let header_height: u16 = if has_summary && inner.height >= 4 {
            2
        } else {
            0
        };
        let chunks =
            Layout::vertical([Constraint::Length(header_height), Constraint::Min(0)]).split(inner);

        if let Some(ref summary) = self.preview_summary
            && header_height > 0
        {
            let programs_str = if summary.programs.is_empty() {
                String::new()
            } else {
                format!(" · {}", summary.programs.join(", "))
            };
            let summary_line = format!(
                " {}w · {}p{}",
                summary.window_count, summary.pane_count, programs_str
            );
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(Span::styled(
                        truncate_str(&summary_line, inner.width as usize),
                        Style::default().fg(theme::CYAN),
                    )),
                    Line::from(Span::styled(
                        "─".repeat(inner.width as usize),
                        Style::default().fg(theme::BLUE),
                    )),
                ]),
                chunks[0],
            );
        }

        let content_area = chunks[1];
        if let Some(ref content) = self.preview_content {
            let lines: Vec<Line> = content
                .lines()
                .map(|l| Line::raw(l.trim_end().to_string()))
                .collect();
            let visible_height = content_area.height as usize;
            let skip = lines.len().saturating_sub(visible_height);
            let visible: Vec<Line> = lines.into_iter().skip(skip).collect();
            frame.render_widget(
                Paragraph::new(visible).style(Style::default().fg(theme::TEXT)),
                content_area,
            );
        } else {
            draw_ascii_art(frame, content_area);
        }
    }
}

fn draw_session_card(frame: &mut Frame, area: Rect, session: &SessionRecord, is_selected: bool) {
    let (status_label, state_color) = if session.claude_session_id.is_none() {
        ("", theme::GRAY_DIM)
    } else {
        let (label, color) = match session.state {
            SessionState::Working => ("working", theme::ORANGE_BRIGHT),
            SessionState::WaitingUser => ("waiting", theme::CYAN_VIVID),
            SessionState::Idle => ("idle", theme::GRAY_DIM),
        };
        (label, color)
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

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(card_inner);

    let status_w = status_label.len();
    let max_name = content_w.saturating_sub(if status_w > 0 { status_w + 1 } else { 0 });
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
    let name_display_w = name.chars().count();
    let gap = content_w.saturating_sub(name_display_w + status_w);
    let padding = " ".repeat(gap);
    let status_style = if matches!(session.state, SessionState::WaitingUser) {
        Style::default()
            .fg(state_color)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(state_color)
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(name, name_style),
            Span::raw(padding),
            Span::styled(status_label, status_style),
        ])),
        rows[0],
    );

    let dir = shorten_path(&session.directory, content_w);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            dir,
            Style::default().fg(theme::GRAY_DIM),
        ))),
        rows[1],
    );

    let cmd_tag = match &session.model_id {
        Some(m) => format!("{} · {}", session.claude_command, shorten_model_id(m)),
        None => session.claude_command.clone(),
    };
    let cmd_tag = truncate_str(&cmd_tag, content_w);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            cmd_tag,
            Style::default().fg(theme::GRAY_DIM),
        ))),
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

    let total_h = (VAN_ART.len() + 1 + DAMME_ART.len() + 2) as u16;
    let start_y = area.y + area.height.saturating_sub(total_h) / 2;
    let mut y = start_y;

    for line in VAN_ART {
        if y >= area.y + area.height {
            break;
        }
        frame.render_widget(
            Paragraph::new(Span::styled(
                *line,
                Style::default().fg(theme::ORANGE_BRIGHT),
            ))
            .alignment(Alignment::Center),
            Rect::new(area.x, y, area.width, 1),
        );
        y += 1;
    }

    y += 1;

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

    y += 2;

    if y < area.y + area.height {
        frame.render_widget(
            Paragraph::new(Span::styled(TAGLINE, Style::default().fg(theme::GRAY_DIM)))
                .alignment(Alignment::Center),
            Rect::new(area.x, y, area.width, 1),
        );
    }
}

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
    fn test_new_selects_first_session() {
        let list = SessionList::new(sample_grouped_sessions());
        assert_eq!(
            list.list.selected_item().unwrap().tmux_session_name,
            "alpha"
        );
    }

    #[test]
    fn test_new_empty_selects_none() {
        let list = SessionList::new(vec![]);
        assert!(list.list.selected_item().is_none());
    }

    #[test]
    fn test_q_quits() {
        let mut list = SessionList::new(sample_grouped_sessions());
        let action = list.handle_key(key(KeyCode::Char('q')));
        assert_eq!(action, SessionListAction::Quit);
    }

    #[test]
    fn test_n_new_task() {
        let mut list = SessionList::new(sample_grouped_sessions());
        let action = list.handle_key(key(KeyCode::Char('n')));
        assert_eq!(action, SessionListAction::NewTask);
    }

    #[test]
    fn test_t_new_tmux_session() {
        let mut list = SessionList::new(sample_grouped_sessions());
        let action = list.handle_key(key(KeyCode::Char('t')));
        assert_eq!(action, SessionListAction::NewTmuxSession);
    }

    #[test]
    fn test_a_attaches_selected() {
        let mut list = SessionList::new(sample_grouped_sessions());
        list.handle_key(key(KeyCode::Down)); // beta
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
    fn test_esc_does_not_quit() {
        let mut list = SessionList::new(sample_grouped_sessions());
        let action = list.handle_key(key(KeyCode::Esc));
        assert_eq!(action, SessionListAction::None);
    }

    #[test]
    fn test_x_sets_confirm_kill() {
        let mut list = SessionList::new(sample_grouped_sessions());
        let action = list.handle_key(key(KeyCode::Char('x')));
        assert_eq!(action, SessionListAction::None);
        assert_eq!(list.confirm_kill, Some("alpha".to_string()));
        assert!(list.status_message.as_ref().unwrap().contains("confirm"));
    }

    #[test]
    fn test_x_on_empty_is_noop() {
        let mut list = SessionList::new(vec![]);
        list.handle_key(key(KeyCode::Char('x')));
        assert_eq!(list.confirm_kill, None);
    }

    #[test]
    fn test_confirm_kill_cancelled_by_other_key() {
        let mut list = SessionList::new(sample_grouped_sessions());
        list.handle_key(key(KeyCode::Char('x')));
        assert!(list.confirm_kill.is_some());
        list.handle_key(key(KeyCode::Esc));
        assert_eq!(list.confirm_kill, None);
        assert_eq!(list.status_message.as_ref().unwrap(), "Kill cancelled.");
    }

    #[test]
    fn test_confirm_kill_cancelled_preserves_sessions() {
        let mut list = SessionList::new(sample_grouped_sessions());
        let count_before = list.sessions().len();
        list.handle_key(key(KeyCode::Char('x')));
        list.handle_key(key(KeyCode::Char('n'))); // cancel
        assert_eq!(list.sessions().len(), count_before);
        assert_eq!(list.confirm_kill, None);
    }

    #[test]
    fn test_navigation_during_confirm_cancels() {
        let mut list = SessionList::new(sample_grouped_sessions());
        list.handle_key(key(KeyCode::Char('x')));
        assert!(list.confirm_kill.is_some());
        list.handle_key(key(KeyCode::Char('j'))); // cancels
        assert_eq!(list.confirm_kill, None);
        assert_eq!(list.status_message.as_ref().unwrap(), "Kill cancelled.");
    }

    #[test]
    fn test_select_by_name() {
        let mut list = SessionList::new(sample_grouped_sessions());
        list.select_by_name("gamma");
        assert_eq!(
            list.list.selected_item().unwrap().tmux_session_name,
            "gamma"
        );
    }

    #[test]
    fn test_select_by_name_falls_back_to_first() {
        let mut list = SessionList::new(sample_grouped_sessions());
        list.select_by_name("nonexistent");
        assert_eq!(
            list.list.selected_item().unwrap().tmux_session_name,
            "alpha"
        );
    }

    #[test]
    fn test_attach_correct_session_across_groups() {
        let mut list = SessionList::new(sample_grouped_sessions());
        list.handle_key(key(KeyCode::Down)); // beta
        list.handle_key(key(KeyCode::Down)); // gamma
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
