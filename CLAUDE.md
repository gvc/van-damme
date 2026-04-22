# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

TUI application to manage tmux and Claude sessions, built with Rust. Binary name: `vd`.

## Architecture

- **`src/main.rs`** — Entry point. Routes `process-hook` subcommand or runs the TUI draw/event loop, dispatching between the session list and new-task screens.
- **`src/app.rs`** — Application state (`App` struct) with a four-field input form (title, directory, prompt, CLI args). Handles keyboard input and returns `Action` variants (`None`, `Quit`, `Submit`). Renders a centered form using ratatui + `tui-input`.
- **`src/session_list.rs`** — The "Active Sessions" screen — navigable list of live sessions with state icons (⚙/⏳/●) and vim-style keybindings.
- **`src/tmux.rs`** — Tmux command wrappers: `create_session` (Claude window only), `setup_editor_window` (editor + split, triggered by SessionStart hook), `kill_session`, session name sanitization, and shell escaping.
- **`src/session.rs`** — JSON persistence layer for `~/.van-damme/sessions.json`. Stores `SessionRecord` entries with `SessionState` (Working, WaitingUser, Idle). Provides lookup by claude session ID and state updates.
- **`src/process_hook.rs`** — Claude Code hook handler. Reads hook event JSON from stdin, updates session state in the DB, and triggers editor window creation on `SessionStart`.
- **`src/event.rs`** — Event abstraction over crossterm. `EventHandler` polls for keyboard input with a configurable tick rate.
- **`src/tui.rs`** — Terminal setup/teardown (raw mode, alternate screen).
- **`src/theme.rs`** — Color palette constants (dark background with warm orange accents).

Key dependencies: `ratatui` (UI framework), `crossterm` (terminal backend), `color-eyre` (error handling), `tui-input` (text input widget), `serde`/`serde_json` (serialization), `dirs` (home directory resolution), `uuid` (Claude session ID generation).

## Build & Test Commands

- **Build:** `cargo build`
- **Run:** `cargo run`
- **Test all:** `cargo test`
- **Test single:** `cargo test test_name`
- **Test single module:** `cargo test --lib module_name`
- **Lint:** `cargo clippy -- -D warnings`
- **Format:** `cargo fmt`
- **Check (fast compile check):** `cargo check`

## Development Rules

- Always write tests for new code. Every new function, module, or feature must include corresponding test code.
- Run `cargo clippy -- -D warnings` before committing to catch lint issues.
- Run `cargo fmt` to ensure consistent formatting.
