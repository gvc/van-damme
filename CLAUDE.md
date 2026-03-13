# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

TUI application for Claude in iTerm, built with Rust. Binary name: `van-damme`.

## Architecture

- **`src/main.rs`** — Entry point and main event loop. Initializes terminal, runs the draw/event cycle, dispatches `Action`s from the app. On `Action::Submit`, restores the terminal and launches a tmux session.
- **`src/app.rs`** — Application state (`App` struct) with a two-field input form (title + directory). Handles keyboard input and returns `Action` variants (`None`, `Quit`, `Submit`). Renders a centered form using ratatui + `tui-input`.
- **`src/tmux.rs`** — Tmux command wrappers: `sanitize_session_name`, `session_exists`, `create_session`. Creates sessions with a Claude window (`claude --worktree`) and an editor window (vim + split pane).
- **`src/session.rs`** — JSON persistence layer for `~/.van-damme/sessions.json`. Stores `SessionRecord` entries with tmux session info, directory, and timestamps.
- **`src/event.rs`** — Event abstraction over crossterm. `EventHandler` polls for keyboard input with a configurable tick rate.
- **`src/tui.rs`** — Terminal setup/teardown (raw mode, alternate screen).

Key dependencies: `ratatui` (UI framework), `crossterm` (terminal backend), `color-eyre` (error handling), `tui-input` (text input widget), `serde`/`serde_json` (serialization), `dirs` (home directory resolution).

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
