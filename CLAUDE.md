# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

TUI application for Claude in iTerm, built with Rust. Binary name: `van-damme`.

## Architecture

- **`src/main.rs`** — Entry point and main event loop. Initializes terminal, runs the draw/event cycle, and handles key dispatch.
- **`src/app.rs`** — Application state (`App` struct) and UI rendering (`draw` method using ratatui widgets).
- **`src/event.rs`** — Event abstraction over crossterm. `EventHandler` polls for keyboard input with a configurable tick rate.
- **`src/tui.rs`** — Terminal setup/teardown (raw mode, alternate screen).

Key dependencies: `ratatui` (UI framework), `crossterm` (terminal backend), `color-eyre` (error handling).

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
