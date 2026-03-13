use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph},
};

#[derive(Debug, Default)]
pub struct App {
    pub running: bool,
}

impl App {
    pub fn new() -> Self {
        Self { running: true }
    }

    pub fn quit(&mut self) {
        self.running = false;
    }

    pub fn draw(&self, frame: &mut Frame) {
        let chunks = Layout::vertical([Constraint::Min(1), Constraint::Length(1)])
            .split(frame.area());

        let main_block = Block::default()
            .title(" van-damme ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let content = Paragraph::new("Press 'q' to quit.")
            .block(main_block);

        frame.render_widget(content, chunks[0]);

        let status = Paragraph::new(" van-damme v0.1.0 ")
            .style(Style::default().fg(Color::DarkGray));

        frame.render_widget(status, chunks[1]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_app_is_running() {
        let app = App::new();
        assert!(app.running);
    }

    #[test]
    fn test_quit_stops_app() {
        let mut app = App::new();
        app.quit();
        assert!(!app.running);
    }

    #[test]
    fn test_default_app_is_not_running() {
        let app = App::default();
        assert!(!app.running);
    }
}
