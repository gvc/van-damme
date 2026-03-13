mod app;
mod event;
mod tui;

use color_eyre::Result;
use crossterm::event::KeyCode;

use app::App;
use event::{Event, EventHandler};

fn main() -> Result<()> {
    color_eyre::install()?;

    let mut terminal = tui::init()?;
    let mut app = App::new();
    let events = EventHandler::new(250);

    while app.running {
        terminal.draw(|frame| app.draw(frame))?;

        match events.next()? {
            Event::Key(key) => handle_key(&mut app, key.code),
            Event::Tick => {}
        }
    }

    tui::restore()?;
    Ok(())
}

fn handle_key(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Char('q') | KeyCode::Esc => app.quit(),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_key_quit_q() {
        let mut app = App::new();
        handle_key(&mut app, KeyCode::Char('q'));
        assert!(!app.running);
    }

    #[test]
    fn test_handle_key_quit_esc() {
        let mut app = App::new();
        handle_key(&mut app, KeyCode::Esc);
        assert!(!app.running);
    }

    #[test]
    fn test_handle_key_other_does_not_quit() {
        let mut app = App::new();
        handle_key(&mut app, KeyCode::Char('a'));
        assert!(app.running);
    }
}
