mod app;
mod event;
mod session;
mod tmux;
mod tui;

use color_eyre::Result;

use app::{Action, App};
use event::{Event, EventHandler};

fn main() -> Result<()> {
    color_eyre::install()?;

    let mut terminal = tui::init()?;
    let mut app = App::new();
    let events = EventHandler::new(250);

    while app.running {
        terminal.draw(|frame| app.draw(frame))?;

        match events.next()? {
            Event::Key(key) => {
                if key.kind == crossterm::event::KeyEventKind::Press {
                    let action = app.handle_key(key);
                    match action {
                        Action::Submit { title, directory } => {
                            // Leave alternate screen before running tmux commands
                            tui::restore()?;

                            if let Err(e) = launch_session(&title, &directory) {
                                eprintln!("Error: {e}");
                                std::process::exit(1);
                            }

                            app.running = false;
                        }
                        Action::Quit => {}
                        Action::None => {}
                    }
                }
            }
            Event::Tick => {}
        }
    }

    // Only restore if we haven't already (Submit path restores early)
    if crossterm::terminal::is_raw_mode_enabled()? {
        tui::restore()?;
    }

    Ok(())
}

fn launch_session(title: &str, directory: &str) -> Result<()> {
    let session_name = tmux::sanitize_session_name(title);

    if session_name.is_empty() {
        return Err(color_eyre::eyre::eyre!(
            "Title '{title}' produces an empty session name"
        ));
    }

    // Check tmux is available
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

    // Check for name collision
    if tmux::session_exists(&session_name)? {
        return Err(color_eyre::eyre::eyre!(
            "tmux session '{session_name}' already exists"
        ));
    }

    let tmux_session = tmux::create_session(&session_name, directory)?;

    session::add_session(
        tmux_session.session_id,
        tmux_session.session_name.clone(),
        directory.to_string(),
    )?;

    println!("Created tmux session: {}", tmux_session.session_name);
    println!("Attach with: tmux attach -t {}", tmux_session.session_name);

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
