mod app;
mod event;
mod recent_dirs;
mod session;
mod session_list;
pub mod theme;
mod tmux;
mod tui;

use color_eyre::Result;
use ratatui::{style::Style, widgets::Block};

use app::{Action, App};
use event::{Event, EventHandler};
use session_list::{SessionList, SessionListAction};

#[derive(Debug)]
enum Screen {
    SessionList,
    NewTask,
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    color_eyre::install()?;

    let mut terminal = tui::init()?;
    let events = EventHandler::new(250);

    let sessions = session::list_sessions().unwrap_or_default();
    // Filter to only sessions still alive in tmux
    let alive: Vec<_> = sessions
        .into_iter()
        .filter(|s| tmux::session_exists(&s.tmux_session_name).unwrap_or(false))
        .collect();

    let mut session_list = SessionList::new(alive);
    let recent_dirs = recent_dirs::recent_directories(5).unwrap_or_default();
    let mut app = App::with_recent_dirs(recent_dirs.clone());
    let mut screen = Screen::SessionList;
    let mut running = true;

    while running {
        terminal.draw(|frame| {
            // Fill entire screen with theme background
            frame.render_widget(
                Block::default().style(Style::default().bg(theme::BG)),
                frame.area(),
            );
            match screen {
                Screen::SessionList => session_list.draw(frame),
                Screen::NewTask => app.draw(frame),
            }
        })?;

        match events.next()? {
            Event::Key(key) => {
                if key.kind == crossterm::event::KeyEventKind::Press {
                    match screen {
                        Screen::SessionList => {
                            let action = session_list.handle_key(key);
                            match action {
                                SessionListAction::Quit => {
                                    running = false;
                                }
                                SessionListAction::NewTask => {
                                    let recent = recent_dirs::recent_directories(5).unwrap_or_default();
                                    app = App::with_recent_dirs(recent);
                                    screen = Screen::NewTask;
                                }
                                SessionListAction::None => {}
                            }
                        }
                        Screen::NewTask => {
                            let action = app.handle_key(key);
                            match action {
                                Action::Submit {
                                    title,
                                    directory,
                                    prompt,
                                    claude_args,
                                } => {
                                    if let Err(e) = launch_session(
                                        &title,
                                        &directory,
                                        prompt.as_deref(),
                                        claude_args.as_deref(),
                                    ) {
                                        app.error_message = Some(format!("{e}"));
                                    } else {
                                        session_list.refresh();
                                        screen = Screen::SessionList;
                                    }
                                }
                                Action::Quit => {
                                    // Go back to session list instead of quitting
                                    session_list.refresh();
                                    screen = Screen::SessionList;
                                }
                                Action::None => {}
                            }
                        }
                    }
                }
            }
            Event::Tick => {}
        }
    }

    if crossterm::terminal::is_raw_mode_enabled()? {
        tui::restore()?;
    }

    Ok(())
}

fn launch_session(
    title: &str,
    directory: &str,
    prompt: Option<&str>,
    claude_args: Option<&str>,
) -> Result<()> {
    let session_name = tmux::sanitize_session_name(title);

    if session_name.is_empty() {
        return Err(color_eyre::eyre::eyre!(
            "Title '{title}' produces an empty session name"
        ));
    }

    if std::process::Command::new("tmux")
        .arg("-V")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_err()
    {
        return Err(color_eyre::eyre::eyre!(
            "tmux is not installed or not in PATH"
        ));
    }

    if tmux::session_exists(&session_name)? {
        return Err(color_eyre::eyre::eyre!(
            "tmux session '{session_name}' already exists"
        ));
    }

    let tmux_session = tmux::create_session(&session_name, directory, prompt, claude_args)?;

    session::add_session(
        tmux_session.session_id,
        tmux_session.session_name.clone(),
        directory.to_string(),
    )?;

    recent_dirs::record_directory(directory)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn test_esc_quits() {
        let mut app = App::new();
        let action = app.handle_key(press(KeyCode::Esc));
        assert_eq!(action, Action::Quit);
        assert!(!app.running);
    }

    #[test]
    fn test_submit_returns_action() {
        let mut app = App::new();
        for ch in "test task".chars() {
            app.handle_key(press(KeyCode::Char(ch)));
        }
        app.handle_key(press(KeyCode::Tab));
        while !app.dir_input.value().is_empty() {
            app.handle_key(press(KeyCode::Backspace));
        }
        for ch in "/tmp".chars() {
            app.handle_key(press(KeyCode::Char(ch)));
        }
        let action = app.handle_key(press(KeyCode::Enter));
        assert!(matches!(action, Action::Submit { .. }));
    }

    #[test]
    fn test_sanitize_produces_valid_name() {
        let name = tmux::sanitize_session_name("My Cool Task!");
        assert_eq!(name, "my-cool-task");
    }
}
