# Van Damme

A terminal UI (TUI) application for managing Claude Code sessions inside tmux. It provides a streamlined workflow for spinning up isolated Claude coding environments — each with its own git worktree, editor window, and Claude instance — and switching between them from a single dashboard.

## What It Does

Van Damme acts as a session manager for Claude Code. When you launch it, you see a list of your active sessions. From there you can:

- **Create a new task** — Fill in a title, working directory, and optional initial prompt. Van Damme creates a tmux session with two windows:
  1. **`claude`** — Runs `claude --worktree <name>` in the specified directory, giving Claude its own isolated git worktree to work in. If you provided an initial prompt, it's passed directly to Claude.
  2. **`editor`** — Opens `vim .` in the worktree directory with a horizontal split pane, so you can review and edit Claude's changes side-by-side.

- **Attach to an existing session** — Select a session and press Enter to jump into the tmux session.

- **Kill a session** — Remove a session you no longer need. This kills the tmux session and cleans up the session record.

All session metadata (tmux IDs, directories, timestamps) is persisted to `~/.van-damme/sessions.json`, so the app remembers your sessions across restarts. On startup, it filters out any sessions whose tmux processes have since been terminated.

## How It Works

### Architecture

The application is structured into six modules:

| Module | Purpose |
|---|---|
| `main.rs` | Entry point. Initializes the terminal, runs the draw/event loop, and dispatches between the session list and new-task screens. |
| `app.rs` | The "New Task" form — a three-field input (title, directory, initial prompt) with tab-completion on the directory field. Validates inputs and emits `Submit` actions. |
| `session_list.rs` | The "Active Sessions" screen — a navigable list of live sessions with vim-style keybindings (j/k, Enter, x, n, q). |
| `tmux.rs` | Tmux command wrappers for creating, checking, and killing sessions. Handles session name sanitization and shell escaping. |
| `session.rs` | JSON persistence layer. Reads/writes `SessionRecord` entries to `~/.van-damme/sessions.json`. |
| `event.rs` | Thin abstraction over crossterm's event polling with a configurable tick rate. |
| `tui.rs` | Terminal setup and teardown (raw mode, alternate screen). |
| `theme.rs` | Color palette constants (dark background with warm orange accents). |

### Screen Flow

```
┌─────────────────────┐       'n'        ┌──────────────────┐
│   Active Sessions   │ ───────────────▸  │    New Task       │
│                     │                   │                   │
│  ▸ task-one  /src   │    Esc (back)     │  Title: [       ] │
│    task-two  /api   │ ◂───────────────  │  Dir:   [       ] │
│                     │                   │  Prompt:[       ] │
│  j/k  Enter  x  q  │                   │  Tab  Enter  Esc  │
└─────────────────────┘                   └──────────────────┘
         │ Enter                                   │ Enter
         ▼                                         ▼
   tmux attach -t <name>              tmux new-session + persist
```

### Directory Autocomplete

The directory input field features filesystem tab-completion. As you type a path, it computes matching directories and displays ghost suggestions in blue. Press the right arrow key to accept a suggestion, or keep typing to narrow the matches. It uses longest-common-prefix logic when multiple directories match.

## Keybindings

### Session List

| Key | Action |
|---|---|
| `j` / `Down` | Move selection down |
| `k` / `Up` | Move selection up |
| `Enter` | Attach to selected session |
| `x` | Kill selected session |
| `n` | Create new task |
| `q` / `Esc` | Quit |

### New Task Form

| Key | Action |
|---|---|
| `Tab` / `Down` | Next field |
| `Shift+Tab` / `Up` | Previous field |
| `Right` (in directory field) | Accept autocomplete suggestion |
| `Enter` | Submit (from any field) |
| `Esc` | Back to session list |

## Requirements

- **Rust** (edition 2024) — for building
- **tmux** — must be installed and available in `$PATH`
- **Claude Code CLI** (`claude`) — must be installed for the coding sessions to work
- A terminal emulator (designed for iTerm on macOS)

## Dependencies

| Crate | Version | Purpose |
|---|---|---|
| [`ratatui`](https://crates.io/crates/ratatui) | 0.29 | TUI framework for rendering widgets, layouts, and styled text |
| [`crossterm`](https://crates.io/crates/crossterm) | 0.28 | Cross-platform terminal manipulation (raw mode, events, alternate screen) |
| [`tui-input`](https://crates.io/crates/tui-input) | 0.11 | Text input widget for ratatui with cursor handling |
| [`color-eyre`](https://crates.io/crates/color-eyre) | 0.6 | Colorized error reporting and `Result` type |
| [`serde`](https://crates.io/crates/serde) | 1 | Serialization framework (with `derive` feature) |
| [`serde_json`](https://crates.io/crates/serde_json) | 1 | JSON serialization for session persistence |
| [`dirs`](https://crates.io/crates/dirs) | 6 | Cross-platform home directory resolution |

### Dev Dependencies

| Crate | Version | Purpose |
|---|---|---|
| [`tempfile`](https://crates.io/crates/tempfile) | 3 | Temporary directories for session persistence tests |

## Build & Run

```bash
# Build
cargo build

# Run
cargo run

# Build optimized release binary
cargo build --release

# Run tests
cargo test

# Lint
cargo clippy -- -D warnings

# Format
cargo fmt
```

## Session Storage

Sessions are stored at `~/.van-damme/sessions.json`. Each record contains:

```json
{
  "tmux_session_id": "$1",
  "tmux_session_name": "my-task",
  "claude_session_id": null,
  "directory": "/path/to/project",
  "created_at": 1700000000
}
```

Session names are derived from the task title by lowercasing, replacing whitespace with hyphens, and stripping special characters (e.g., "My Cool Task!" becomes `my-cool-task`).
