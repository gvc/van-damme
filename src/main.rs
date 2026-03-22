mod app;
mod event;
mod git;
mod process_hook;
mod recent_dirs;
mod session;
mod session_list;
pub mod theme;
mod tmux;
mod tui;

use std::sync::mpsc;
use std::thread;

use color_eyre::Result;
use ratatui::layout::{Constraint, Flex, Layout};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use app::{Action, App};
use event::{Event, EventHandler};
use session_list::{SessionList, SessionListAction};

#[derive(Debug)]
enum Screen {
    SessionList,
    NewTask,
    Launching,
}

struct LaunchState {
    session_name: String,
    progress_rx: mpsc::Receiver<String>,
    result_rx: mpsc::Receiver<Result<(), String>>,
    messages: Vec<String>,
    tick: usize,
}

const SPINNER: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if args.get(1).is_some_and(|a| a == "process-hook") {
        return process_hook::run();
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
    let mut launch_state: Option<LaunchState> = None;
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
                Screen::Launching => {
                    if let Some(ref state) = launch_state {
                        draw_launching(frame, state);
                    }
                }
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
                                    let recent =
                                        recent_dirs::recent_directories(5).unwrap_or_default();
                                    app = App::with_recent_dirs(recent);
                                    screen = Screen::NewTask;
                                }
                                SessionListAction::Attach { session_name } => {
                                    tui::restore()?;
                                    let _ = tmux::switch_to_session(&session_name);
                                    terminal = tui::init()?;
                                    session_list.refresh();
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
                                    git_mode,
                                    branch_name,
                                    prompt,
                                    claude_args,
                                } => {
                                    let session_name = tmux::sanitize_session_name(&title);
                                    let state = spawn_launch(
                                        session_name,
                                        title,
                                        directory,
                                        git_mode,
                                        branch_name,
                                        prompt,
                                        claude_args,
                                    );
                                    launch_state = Some(state);
                                    screen = Screen::Launching;
                                }
                                Action::Quit => {
                                    // Go back to session list instead of quitting
                                    session_list.refresh();
                                    screen = Screen::SessionList;
                                }
                                Action::None => {}
                            }
                        }
                        Screen::Launching => {
                            // Ignore keys while launching
                        }
                    }
                }
            }
            Event::Tick => match screen {
                Screen::SessionList => {
                    session_list.refresh_states();
                }
                Screen::Launching => {
                    if let Some(ref mut state) = launch_state {
                        state.tick += 1;

                        // Drain progress messages
                        while let Ok(msg) = state.progress_rx.try_recv() {
                            state.messages.push(msg);
                        }

                        // Check if the launch finished
                        match state.result_rx.try_recv() {
                            Ok(result) => {
                                let session_name = state.session_name.clone();
                                launch_state = None;
                                match result {
                                    Ok(()) => {
                                        session_list.refresh();
                                        session_list.select_by_name(&session_name);
                                        screen = Screen::SessionList;
                                    }
                                    Err(e) => {
                                        app.error_message = Some(e);
                                        screen = Screen::NewTask;
                                    }
                                }
                            }
                            Err(mpsc::TryRecvError::Disconnected) => {
                                // Thread panicked or exited without sending
                                launch_state = None;
                                app.error_message =
                                    Some("Session launch failed unexpectedly".into());
                                screen = Screen::NewTask;
                            }
                            Err(mpsc::TryRecvError::Empty) => {
                                // Still running, keep waiting
                            }
                        }
                    }
                }
                _ => {}
            },
        }
    }

    if crossterm::terminal::is_raw_mode_enabled()? {
        tui::restore()?;
    }

    Ok(())
}

fn draw_launching(frame: &mut ratatui::Frame, state: &LaunchState) {
    let area = frame.area();

    let box_width = 60u16.min(area.width.saturating_sub(4));
    let content_lines = state.messages.len() as u16 + 1; // +1 for spinner line
    let box_height = (content_lines + 2)
        .min(area.height.saturating_sub(4))
        .max(5); // +2 for borders

    let [vert_area] = Layout::vertical([Constraint::Length(box_height)])
        .flex(Flex::Center)
        .areas(area);
    let [centered] = Layout::horizontal([Constraint::Length(box_width)])
        .flex(Flex::Center)
        .areas(vert_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ORANGE))
        .title(" Launching Session ")
        .title_style(Style::default().fg(theme::ORANGE_BRIGHT))
        .style(Style::default().bg(theme::BG));

    let spinner_char = SPINNER[state.tick % SPINNER.len()];

    let mut lines: Vec<Line> = state
        .messages
        .iter()
        .map(|msg| {
            Line::from(vec![
                Span::styled("  ✓ ", Style::default().fg(theme::ORANGE)),
                Span::styled(msg.clone(), Style::default().fg(theme::TEXT)),
            ])
        })
        .collect();

    // Add the current spinner line
    let spinner_text = if state.messages.is_empty() {
        "Starting...".to_string()
    } else {
        "Working...".to_string()
    };
    lines.push(Line::from(vec![
        Span::styled(
            format!("  {spinner_char} "),
            Style::default().fg(theme::ORANGE_BRIGHT),
        ),
        Span::styled(spinner_text, Style::default().fg(theme::GRAY_DIM)),
    ]));

    let paragraph = Paragraph::new(lines)
        .block(block)
        .style(Style::default().bg(theme::BG));
    frame.render_widget(paragraph, centered);
}

fn spawn_launch(
    session_name: String,
    title: String,
    directory: String,
    git_mode: app::GitMode,
    branch_name: Option<String>,
    prompt: Option<String>,
    claude_args: Option<String>,
) -> LaunchState {
    let (progress_tx, progress_rx) = mpsc::channel();
    let (result_tx, result_rx) = mpsc::channel();
    thread::spawn(move || {
        let result = launch_session(
            &title,
            &directory,
            git_mode,
            branch_name.as_deref(),
            prompt.as_deref(),
            claude_args.as_deref(),
            &progress_tx,
        );
        let _ = result_tx.send(result.map_err(|e| format!("{e}")));
    });

    LaunchState {
        session_name,
        progress_rx,
        result_rx,
        messages: Vec::new(),
        tick: 0,
    }
}

fn launch_session(
    title: &str,
    directory: &str,
    git_mode: app::GitMode,
    branch_name: Option<&str>,
    prompt: Option<&str>,
    claude_args: Option<&str>,
    progress: &mpsc::Sender<String>,
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

    // Prepare git state before launching Claude
    let use_worktree = git_mode == app::GitMode::Worktree;
    match git_mode {
        app::GitMode::Worktree => {
            git::prepare_worktree(directory, |step| {
                let _ = progress.send(step.to_string());
            })?;
        }
        app::GitMode::Branch => {
            if let Some(branch) = branch_name {
                let _ = progress.send(format!("Preparing branch '{branch}'..."));
                git::prepare_branch(directory, branch)?;
                let _ = progress.send(format!("Branch '{branch}' ready"));
            }
        }
    }

    // Generate the claude session UUID and persist the record BEFORE creating the
    // tmux session. Claude's SessionStart hook fires immediately on launch and needs
    // the record to already exist in the DB to set up the editor window.
    let claude_session_id = uuid::Uuid::new_v4().to_string();

    let _ = progress.send("Creating tmux session...".into());

    session::add_session(
        String::new(), // placeholder — updated after tmux session is created
        session_name.clone(),
        claude_session_id.clone(),
        directory.to_string(),
    )?;

    let tmux_session = match tmux::create_session(
        &session_name,
        directory,
        prompt,
        claude_args,
        &claude_session_id,
        use_worktree,
    ) {
        Ok(s) => s,
        Err(e) => {
            // Clean up the DB record if tmux session creation fails
            let _ = session::remove_session(&session_name);
            return Err(e);
        }
    };

    // Update the record with the real tmux session ID
    session::update_tmux_session_id(&session_name, &tmux_session.session_id)?;

    recent_dirs::record_directory(directory)?;

    let _ = progress.send("Session launched".into());

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

    #[test]
    fn test_launch_state_spinner_cycles() {
        let (_ptx, prx) = mpsc::channel();
        let (_rtx, rrx) = mpsc::channel();
        let state = LaunchState {
            session_name: "test".into(),
            progress_rx: prx,
            result_rx: rrx,
            messages: vec!["Step 1 done".into()],
            tick: 0,
        };
        assert_eq!(SPINNER[state.tick % SPINNER.len()], '⠋');
    }

    #[test]
    fn test_launch_state_receives_progress() {
        let (ptx, prx) = mpsc::channel();
        let (_rtx, rrx) = mpsc::channel();
        let mut state = LaunchState {
            session_name: "test".into(),
            progress_rx: prx,
            result_rx: rrx,
            messages: Vec::new(),
            tick: 0,
        };

        ptx.send("Step 1".into()).unwrap();
        ptx.send("Step 2".into()).unwrap();

        while let Ok(msg) = state.progress_rx.try_recv() {
            state.messages.push(msg);
        }

        assert_eq!(state.messages, vec!["Step 1", "Step 2"]);
    }
}
