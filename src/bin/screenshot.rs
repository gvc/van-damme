//! Generate SVG screenshots of the TUI screens for the README.
//!
//! Usage: cargo run --bin screenshot

use ratatui::{
    Terminal,
    backend::TestBackend,
    buffer::Buffer,
    layout::{Constraint, Flex, Layout, Position},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
};
use std::fmt::Write as FmtWrite;

// Inline theme constants (mirrors src/theme.rs)
mod theme {
    use ratatui::style::Color;
    pub const BG: Color = Color::Rgb(53, 56, 63);
    pub const ORANGE: Color = Color::Rgb(200, 90, 26);
    pub const ORANGE_BRIGHT: Color = Color::Rgb(220, 120, 40);
    pub const BLUE: Color = Color::Rgb(74, 106, 138);
    pub const GRAY: Color = Color::Rgb(60, 60, 80);
    pub const GRAY_DIM: Color = Color::Rgb(65, 137, 181);
    pub const TEXT: Color = Color::Rgb(180, 180, 195);
    pub const SESSION_NAME: Color = Color::Rgb(249, 217, 67);
}

fn color_to_css(color: Color) -> String {
    match color {
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        Color::Reset => color_to_css(theme::BG),
        _ => "#b4b4c3".to_string(),
    }
}

fn buffer_to_svg(buf: &Buffer, width: u16, height: u16) -> String {
    let cell_w = 9.6;
    let cell_h = 20.0;
    let pad_x = 16.0;
    let pad_y = 16.0;
    let svg_w = (width as f64) * cell_w + pad_x * 2.0;
    let svg_h = (height as f64) * cell_h + pad_y * 2.0;
    let corner = 8.0;

    let mut svg = String::new();
    let _ = write!(
        svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {svg_w} {svg_h}" width="{svg_w}" height="{svg_h}">
<rect width="{svg_w}" height="{svg_h}" rx="{corner}" ry="{corner}" fill="{}"/>
<style>text {{ font-family: 'JetBrains Mono', 'Fira Code', 'SF Mono', 'Cascadia Code', Menlo, Monaco, monospace; font-size: 13.5px; }}</style>
"#,
        color_to_css(theme::BG)
    );

    for y in 0..height {
        for x in 0..width {
            let cell = &buf[(x, y)];
            let symbol = cell.symbol();
            if symbol.trim().is_empty() && cell.bg == Color::Reset {
                continue;
            }

            // Background
            let bg = if cell.bg != Color::Reset {
                cell.bg
            } else {
                theme::BG
            };
            if bg != theme::BG {
                let _ = write!(
                    svg,
                    r#"<rect x="{}" y="{}" width="{}" height="{}" fill="{}"/>"#,
                    pad_x + (x as f64) * cell_w,
                    pad_y + (y as f64) * cell_h,
                    cell_w + 0.5,
                    cell_h,
                    color_to_css(bg)
                );
            }

            if symbol.trim().is_empty() {
                continue;
            }

            let fg = if cell.fg != Color::Reset {
                cell.fg
            } else {
                theme::TEXT
            };

            let bold = cell.modifier.contains(Modifier::BOLD);
            let escaped = match symbol {
                "<" => "&lt;".to_string(),
                ">" => "&gt;".to_string(),
                "&" => "&amp;".to_string(),
                "\"" => "&quot;".to_string(),
                other => other.to_string(),
            };

            let _ = write!(
                svg,
                r#"<text x="{}" y="{}" fill="{}"{}>{}  </text>"#,
                pad_x + (x as f64) * cell_w,
                pad_y + (y as f64) * cell_h + cell_h * 0.75,
                color_to_css(fg),
                if bold { r#" font-weight="bold""# } else { "" },
                escaped,
            );
        }
    }

    svg.push_str("</svg>\n");
    svg
}

fn render_session_list() -> String {
    let width = 80;
    let height = 24;
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            let area = frame.area();

            // Fill background
            frame.render_widget(Block::default().style(Style::default().bg(theme::BG)), area);

            let form_width = 70u16.min(area.width.saturating_sub(2));
            let form_height = 20u16.min(area.height.saturating_sub(2));

            let vertical = Layout::vertical([Constraint::Length(form_height)])
                .flex(Flex::Center)
                .split(area);
            let horizontal = Layout::horizontal([Constraint::Length(form_width)])
                .flex(Flex::Center)
                .split(vertical[0]);
            let panel_area = horizontal[0];

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
                Constraint::Min(1),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .split(inner);

            // Sample sessions
            let sessions = [
                ("fix-auth-middleware", "/home/dev/api-server"),
                ("add-search-feature", "/home/dev/webapp"),
                ("refactor-database", "/home/dev/backend"),
                ("update-ci-pipeline", "/home/dev/infra"),
            ];

            let items: Vec<ListItem> = sessions
                .iter()
                .map(|(name, dir)| {
                    let line = Line::from(vec![
                        Span::styled(
                            *name,
                            Style::default()
                                .fg(theme::SESSION_NAME)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::raw("  "),
                        Span::styled(*dir, Style::default().fg(theme::GRAY_DIM)),
                    ]);
                    ListItem::new(line)
                })
                .collect();

            let mut list_state = ListState::default();
            list_state.select(Some(0));

            let list = List::new(items)
                .highlight_style(
                    Style::default()
                        .bg(theme::GRAY)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol("▸ ");

            frame.render_stateful_widget(list, chunks[0], &mut list_state);

            let hints = Paragraph::new(Line::from(Span::styled(
                "j/k: navigate  |  Enter: attach  |  x: kill  |  n: new  |  q: quit",
                Style::default().fg(theme::GRAY_DIM),
            )));
            frame.render_widget(hints, chunks[1]);
        })
        .unwrap();

    let buf = terminal.backend().buffer().clone();
    buffer_to_svg(&buf, width, height)
}

fn render_new_task() -> String {
    let width = 80;
    let height = 24;
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            let area = frame.area();

            frame.render_widget(Block::default().style(Style::default().bg(theme::BG)), area);

            let form_width = 60u16.min(area.width.saturating_sub(2));
            let form_height = 16u16.min(area.height.saturating_sub(2));

            let vertical = Layout::vertical([Constraint::Length(form_height)])
                .flex(Flex::Center)
                .split(area);
            let horizontal = Layout::horizontal([Constraint::Length(form_width)])
                .flex(Flex::Center)
                .split(vertical[0]);
            let form_area = horizontal[0];

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

            let chunks = Layout::vertical([
                Constraint::Length(1), // Title label
                Constraint::Length(3), // Title input
                Constraint::Length(1), // Directory label
                Constraint::Length(3), // Directory input
                Constraint::Length(1), // Prompt label
                Constraint::Length(3), // Prompt input
                Constraint::Min(1),    // Hints
            ])
            .split(inner);

            // Title label
            let title_label =
                Paragraph::new("Title:").style(Style::default().fg(theme::TEXT).bg(theme::BG));
            frame.render_widget(title_label, chunks[0]);

            // Title input (focused)
            let title_block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme::ORANGE_BRIGHT))
                .style(Style::default().bg(theme::BG));
            let title_para = Paragraph::new("fix-auth-middleware")
                .style(Style::default().fg(theme::TEXT))
                .block(title_block);
            frame.render_widget(title_para, chunks[1]);

            // Directory label
            let dir_label =
                Paragraph::new("Directory:").style(Style::default().fg(theme::TEXT).bg(theme::BG));
            frame.render_widget(dir_label, chunks[2]);

            // Directory input (unfocused, with ghost suggestion)
            let dir_block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme::GRAY))
                .style(Style::default().bg(theme::BG));
            let dir_line = Line::from(vec![
                Span::styled("/home/dev/api-", Style::default().fg(theme::TEXT)),
                Span::styled("server/", Style::default().fg(theme::BLUE)),
            ]);
            let dir_para = Paragraph::new(dir_line).block(dir_block);
            frame.render_widget(dir_para, chunks[3]);

            // Prompt label
            let prompt_label = Paragraph::new("Initial prompt (optional):")
                .style(Style::default().fg(theme::TEXT).bg(theme::BG));
            frame.render_widget(prompt_label, chunks[4]);

            // Prompt input (unfocused)
            let prompt_block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme::GRAY))
                .style(Style::default().bg(theme::BG));
            let prompt_para = Paragraph::new("fix the JWT token validation bug")
                .style(Style::default().fg(theme::TEXT))
                .block(prompt_block);
            frame.render_widget(prompt_para, chunks[5]);

            // Hints
            let hints = Paragraph::new(Line::from(Span::styled(
                "Tab: next field  |  Enter: submit  |  Esc: back",
                Style::default().fg(theme::GRAY_DIM),
            )));
            frame.render_widget(hints, chunks[6]);

            // Cursor
            let title_inner = Block::default().borders(Borders::ALL).inner(chunks[1]);
            frame.set_cursor_position(Position::new(
                title_inner.x + 18, // after "fix-auth-middleware"
                title_inner.y,
            ));
        })
        .unwrap();

    let buf = terminal.backend().buffer().clone();
    buffer_to_svg(&buf, width, height)
}

fn main() {
    let screenshots_dir = std::path::Path::new("screenshots");
    std::fs::create_dir_all(screenshots_dir).unwrap();

    let session_list_svg = render_session_list();
    std::fs::write(screenshots_dir.join("session-list.svg"), &session_list_svg).unwrap();
    println!("Generated screenshots/session-list.svg");

    let new_task_svg = render_new_task();
    std::fs::write(screenshots_dir.join("new-task.svg"), &new_task_svg).unwrap();
    println!("Generated screenshots/new-task.svg");
}
