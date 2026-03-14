# Van Damme: A Deep-Dive Manual

This manual walks through every line of the **van-damme** codebase — a terminal UI (TUI) application written in Rust. It assumes you have no prior Rust experience. When a Rust concept appears for the first time, it's explained in place, with links to [The Rust Programming Language](https://doc.rust-lang.org/book/) book for further reading.

---

## Table of Contents

1. [What the Application Does](#1-what-the-application-does)
2. [Project Structure](#2-project-structure)
3. [Cargo.toml — The Project Manifest](#3-cargotoml--the-project-manifest)
4. [src/main.rs — The Entry Point](#4-srcmainrs--the-entry-point)
5. [src/tui.rs — Terminal Setup and Teardown](#5-srctui-rs--terminal-setup-and-teardown)
6. [src/event.rs — Keyboard and Tick Events](#6-srceventrs--keyboard-and-tick-events)
7. [src/theme.rs — The Color Palette](#7-srcthemers--the-color-palette)
8. [src/app.rs — The "New Task" Form](#8-srcapprs--the-new-task-form)
9. [src/session_list.rs — The Session Browser](#9-srcsession_listrs--the-session-browser)
10. [src/tmux.rs — Talking to tmux](#10-srctmuxrs--talking-to-tmux)
11. [src/session.rs — Persisting Data to JSON](#11-srcsessionrs--persisting-data-to-json)
12. [src/process_hook.rs — Claude Code Hooks](#12-srcprocess_hookrs--claude-code-hooks)
13. [src/recent_dirs.rs — Recent Directories](#13-srcrecent_dirsrs--recent-directories)
14. [How It All Fits Together](#14-how-it-all-fits-together)
15. [Testing](#15-testing)
16. [Glossary of Rust Concepts](#16-glossary-of-rust-concepts)

---

## 1. What the Application Does

Van Damme is a session manager for Claude Code in tmux. It presents a full-screen terminal interface where you can:

- **Browse** your active coding sessions, with live state indicators (working, waiting for user, idle)
- **Create** a new session (which spawns a tmux window running Claude and another running your editor)
- **Attach** to an existing session
- **Kill** sessions you no longer need (including worktree cleanup)

It also integrates with Claude Code's hook system to track session state in real time. When Claude is working, waiting for permission, or idle, the session list reflects that with status icons (⚙/⏳/●).

All session data is persisted to `~/.van-damme/sessions.json`, and recently used directories are tracked in `~/.van-damme/recent_dirs.json`.

---

## 2. Project Structure

```
van-damme/
├── Cargo.toml              # Project manifest and dependencies
└── src/
    ├── main.rs             # Entry point, subcommand routing, main loop, screen transitions
    ├── tui.rs              # Terminal initialization and cleanup
    ├── event.rs            # Keyboard event polling
    ├── theme.rs            # Color constants
    ├── app.rs              # "New Task" form (4-field input, validation, rendering, recent dirs dropdown)
    ├── session_list.rs     # Session list screen (navigation, state icons, rendering)
    ├── tmux.rs             # Shell commands to create/kill tmux sessions and manage worktrees
    ├── session.rs          # JSON file read/write for session records with state tracking
    ├── process_hook.rs     # Claude Code hook handler (reads events from stdin, updates session state)
    ├── recent_dirs.rs      # Recent directories tracking (JSON persistence)
    └── bin/
        └── screenshot.rs   # SVG screenshot generator for documentation
```

Each `.rs` file in `src/` is a **module**. In Rust, the code is organized into modules, which are roughly equivalent to files or namespaces in other languages. The `main.rs` file declares all the other modules and is the only file the compiler starts reading from.

> **Rust concept: Modules**
> Modules group related code together. When you write `mod app;` in `main.rs`, the compiler looks for either `src/app.rs` or `src/app/mod.rs` and treats everything inside as the `app` module.
> [Read more: Modules](https://doc.rust-lang.org/book/ch07-02-defining-modules-to-control-scope-and-privacy.html)

---

## 3. Cargo.toml — The Project Manifest

```toml
[package]
name = "van-damme"
version = "0.3.2"
edition = "2024"

[dependencies]
ratatui = "0.29"
crossterm = "0.28"
color-eyre = "0.6"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
dirs = "6"
tui-input = "0.11"
uuid = { version = "1", features = ["v4"] }

[dev-dependencies]
tempfile = "3"
```

This is the equivalent of `package.json` (JavaScript) or `pyproject.toml` (Python). Cargo is Rust's build tool and package manager.

**What each dependency does:**

| Crate | Purpose |
|---|---|
| `ratatui` | Framework for drawing text-based UIs (widgets, layouts, styles) |
| `crossterm` | Low-level terminal control: raw mode, keyboard input, alternate screen |
| `color-eyre` | Colorful, structured error reporting |
| `serde` + `serde_json` | Serialization: converting Rust structs to/from JSON |
| `dirs` | Cross-platform way to find `~` (home directory) |
| `tui-input` | A text input widget that works with ratatui |
| `uuid` | Generates v4 UUIDs for Claude session IDs |
| `tempfile` (dev only) | Creates temporary files and directories for tests |

> **Rust concept: Crates**
> A "crate" is Rust's term for a package or library. `[dependencies]` lists crates your code uses at runtime. `[dev-dependencies]` lists crates only needed for testing.
> [Read more: Cargo and Crates](https://doc.rust-lang.org/book/ch01-03-hello-cargo.html)

The `edition = "2024"` line sets which version of the Rust language to use. Editions introduce new syntax and features without breaking old code.

The `features = ["derive"]` on serde enables a macro that auto-generates serialization code for your structs (more on this in the session module). The `features = ["v4"]` on uuid enables random UUID generation.

---

## 4. src/main.rs — The Entry Point

This is where the program starts. Let's walk through it section by section.

### 4.1 Module Declarations

```rust
mod app;
mod event;
mod process_hook;
mod recent_dirs;
mod session;
mod session_list;
pub mod theme;
mod tmux;
mod tui;
```

Each `mod` line tells the compiler: "there's a module with this name — go find the `.rs` file and include it." The `pub` on `theme` makes it accessible from outside this crate (although for a binary crate like this one, it mainly means other modules can use `crate::theme` paths — all `mod` declarations in `main.rs` are accessible to sibling modules regardless).

Notice the two new modules compared to the original codebase: `process_hook` handles Claude Code hook events, and `recent_dirs` tracks recently used directories.

### 4.2 Imports

```rust
use color_eyre::Result;
use ratatui::{style::Style, widgets::Block};

use app::{Action, App};
use event::{Event, EventHandler};
use session_list::{SessionList, SessionListAction};
```

> **Rust concept: `use` statements**
> `use` brings names into scope so you can write `Result` instead of `color_eyre::Result` everywhere. The curly braces `{Action, App}` import multiple items from the same path.
> [Read more: use keyword](https://doc.rust-lang.org/book/ch07-04-bringing-paths-into-scope-with-the-use-keyword.html)

### 4.3 The Screen Enum

```rust
#[derive(Debug)]
enum Screen {
    SessionList,
    NewTask,
}
```

This defines the two screens the app can show. An `enum` in Rust is a type that can be exactly one of several variants — like a tagged union. Here, a `Screen` value is either `SessionList` or `NewTask`, never both, never something else.

The `#[derive(Debug)]` above the enum is an **attribute** that auto-generates code to print the value for debugging (like `println!("{:?}", screen)` would output `SessionList`).

> **Rust concept: Enums**
> Enums are one of Rust's most powerful features. Unlike enums in languages like Java or TypeScript (which are just named integers or strings), Rust enums can carry data inside each variant. You'll see this heavily used throughout the codebase.
> [Read more: Enums](https://doc.rust-lang.org/book/ch06-01-defining-an-enum.html)

### 4.4 The Main Function

```rust
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
```

**`fn main() -> Result<()>`** — The entry point returns a `Result`. In Rust, functions that can fail return `Result<T, E>`, where `T` is the success type and `E` is the error type. Here, `Result<()>` means "either succeed with nothing (`()`, pronounced 'unit', Rust's void) or fail with an error."

Before launching the TUI, main handles two special cases:

1. **`--version` / `-V`** — Prints the version from `Cargo.toml` using the `env!` macro, which reads build-time environment variables. `CARGO_PKG_NAME` and `CARGO_PKG_VERSION` are set by Cargo automatically.

2. **`process-hook` subcommand** — If the first argument is `process-hook`, it delegates to the hook handler module instead of launching the TUI. This is how Claude Code communicates session state changes — it calls `van-damme process-hook` and pipes JSON to stdin.

**`.is_some_and(|a| a == "process-hook")`** — A method on `Option` that checks both that the value exists and satisfies a predicate. It's equivalent to `match args.get(1) { Some(a) if a == "process-hook" => true, _ => false }` but much more concise.

**`color_eyre::install()?`** — Sets up fancy error reporting. The `?` operator is critical to understand:

> **Rust concept: The `?` operator**
> When a function returns `Result`, you can put `?` after any expression that also returns a `Result`. If that expression is an error, the function immediately returns that error to its caller. If it's a success, you get the inner value. It's shorthand for "if this fails, bail out."
> [Read more: The ? operator](https://doc.rust-lang.org/book/ch09-02-recoverable-errors-with-result.html#a-shortcut-for-propagating-errors-the--operator)

**`let mut terminal = tui::init()?;`** — Creates the terminal. `let` declares a variable. `mut` makes it mutable (changeable). By default, all variables in Rust are immutable — you must explicitly opt into mutation.

> **Rust concept: Mutability**
> `let x = 5;` creates an immutable binding — you cannot do `x = 6`. To allow changes, write `let mut x = 5;`. This is a deliberate design choice that prevents accidental mutation bugs.
> [Read more: Variables and Mutability](https://doc.rust-lang.org/book/ch03-01-variables-and-mutability.html)

### 4.5 Loading Sessions

```rust
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
```

This loads saved sessions from disk, then filters them down to only those that are actually still running in tmux. It also loads the 5 most recently used directories for the new-task form.

**`unwrap_or_default()`** — If loading fails, use an empty list instead of crashing.

**`Vec<_>`** — A `Vec` is Rust's growable array (like `ArrayList` in Java or a regular array in JavaScript/Python). The `_` tells the compiler "figure out the element type yourself."

**`.into_iter().filter(...).collect()`** — This is an **iterator chain**, the Rust equivalent of JavaScript's `.filter()` or Python's list comprehension. Let's break it down:

- `.into_iter()` — Converts the `Vec` into an iterator (a lazy sequence of values)
- `.filter(|s| ...)` — Keeps only elements where the closure returns `true`
- `.collect()` — Gathers results back into a collection (here, a new `Vec`)

**`|s|`** — This is closure syntax. Closures are anonymous functions. `|s|` declares a parameter `s`. The body follows.

> **Rust concept: Iterators and closures**
> Iterators are Rust's approach to processing sequences. They're lazy (nothing happens until you call `.collect()` or similar), and the compiler optimizes them to be as fast as hand-written loops.
> [Read more: Iterators](https://doc.rust-lang.org/book/ch13-02-iterators.html)
> [Read more: Closures](https://doc.rust-lang.org/book/ch13-01-closures.html)

**`App::with_recent_dirs(recent_dirs.clone())`** — Creates the new-task form pre-loaded with recent directories. The `.clone()` creates a deep copy because the `Vec` will be needed again later.

### 4.6 The Main Loop

```rust
while running {
    terminal.draw(|frame| {
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
                            SessionListAction::Quit => running = false,
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
                            Action::Submit { title, directory, prompt, claude_args } => {
                                // ... launch session or show error
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
        Event::Tick => {
            if matches!(screen, Screen::SessionList) {
                session_list.refresh_states();
            }
        }
    }
}
```

This is the heartbeat of the application. Every TUI follows the same pattern: **draw, wait for input, update state, repeat**.

1. **Draw** — `terminal.draw(|frame| { ... })` gives you a `frame` to paint widgets onto. The closure fills the whole screen with the background color, then delegates to whichever screen is active.

2. **Wait for input** — `events.next()?` blocks until a key is pressed or the tick timer fires.

3. **Update state** — The key event is dispatched to the current screen's `handle_key` method, which returns an `Action` describing what should happen.

4. **Tick handler** — When no key is pressed within 250ms, a `Tick` event fires. If the session list is visible, `refresh_states()` re-reads session states from the database. This is a lightweight operation — it doesn't spawn tmux processes to check liveness, it just reads the JSON file that the hook handler updates.

**`matches!(screen, Screen::SessionList)`** — A convenience macro that returns `true` if the value matches the pattern. Equivalent to `match screen { Screen::SessionList => true, _ => false }`.

> **Rust concept: `match`**
> `match` is Rust's pattern matching — like a `switch` statement on steroids. Unlike `switch`, `match` is **exhaustive**: the compiler forces you to handle every possible variant of an enum. If you add a new variant to `Action` but forget to handle it in a `match`, your code won't compile. This eliminates entire categories of bugs.
> [Read more: match](https://doc.rust-lang.org/book/ch06-02-match.html)

Key behaviors in the main loop:

- **NewTask** on the session list creates a fresh `App` with up-to-date recent directories
- **Attach** exits the TUI, switches the tmux client to the session, then re-enters the TUI and refreshes
- **Quit** from the new-task form goes back to the session list (not exit). Only Quit from the session list exits the program
- **Submit** launches the session, and on success selects it in the list with `session_list.select_by_name()`

Notice the pattern `Action::Submit { title, directory, prompt, claude_args }` — this **destructures** the enum variant, pulling out all four fields into local variables. This is how Rust's enums carry data: the variant holds fields, and `match` extracts them.

### 4.7 Launching Sessions

```rust
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

    // Generate the claude session UUID and persist the record BEFORE creating the
    // tmux session. Claude's SessionStart hook fires immediately on launch and needs
    // the record to already exist in the DB to set up the editor window.
    let claude_session_id = uuid::Uuid::new_v4().to_string();

    session::add_session(
        String::new(), // placeholder — updated after tmux session is created
        session_name.clone(),
        claude_session_id.clone(),
        directory.to_string(),
    )?;

    let tmux_session = match tmux::create_session(
        &session_name, directory, prompt, claude_args, &claude_session_id,
    ) {
        Ok(s) => s,
        Err(e) => {
            let _ = session::remove_session(&session_name);
            return Err(e);
        }
    };

    session::update_tmux_session_id(&session_name, &tmux_session.session_id)?;
    recent_dirs::record_directory(directory)?;

    Ok(())
}
```

**`&str`** — A **string slice**, which is a reference (a borrowed pointer) to string data. The `&` means "I'm borrowing this data, I don't own it."

> **Rust concept: Ownership and borrowing**
> This is Rust's defining feature. Every piece of data has exactly one **owner**. When you pass data to a function, you either:
> - **Move** it (transfer ownership — the caller can no longer use it)
> - **Borrow** it with `&` (the function can read it, but the caller keeps ownership)
> - **Mutably borrow** it with `&mut` (the function can modify it, exclusively)
>
> This system prevents data races, use-after-free bugs, and double-free bugs — all at compile time, with no garbage collector.
> [Read more: Ownership](https://doc.rust-lang.org/book/ch04-01-what-is-ownership.html)
> [Read more: References and Borrowing](https://doc.rust-lang.org/book/ch04-02-references-and-borrowing.html)

**`Option<&str>`** — `Option` is how Rust handles nullable values. Instead of allowing any variable to be `null` (which causes countless bugs in other languages), Rust forces you to use `Option<T>`, which is an enum with two variants: `Some(value)` or `None`. The compiler won't let you use the inner value without first checking whether it exists.

> **Rust concept: Option**
> `Option<T>` is defined as `enum Option<T> { Some(T), None }`. It replaces null. You must explicitly handle the "nothing" case before accessing the value. This is why Rust programs almost never crash from null pointer errors.
> [Read more: Option](https://doc.rust-lang.org/book/ch06-01-defining-an-enum.html#the-option-enum-and-its-advantages-over-null-values)

The launch sequence is carefully ordered to handle a **race condition**:

1. **Generate a UUID** — This becomes the Claude session ID, used to correlate hook events with the right session record.
2. **Persist the record FIRST** — The database entry is created with an empty `tmux_session_id` placeholder. This is critical because Claude's `SessionStart` hook fires immediately when the tmux session starts, and the hook handler needs to find this record to set up the editor window.
3. **Create the tmux session** — If this fails, the placeholder record is cleaned up.
4. **Update the real tmux session ID** — Now that tmux has assigned an ID like `$42`, the record is updated with it.
5. **Record the directory** — Adds it to recent directories for future use.

The `eyre!()` macro creates a rich error value with a message. Macros in Rust are invoked with `!` and can generate code at compile time.

### 4.8 Cleanup

```rust
if crossterm::terminal::is_raw_mode_enabled()? {
    tui::restore()?;
}

Ok(())
```

Before exiting, the program checks if raw mode is still active and restores the terminal to normal. The final `Ok(())` means "the function succeeded, returning nothing." In Rust, the last expression in a function (without a semicolon) is the return value.

> **Rust concept: Implicit returns**
> In Rust, the last expression in a block is its return value. `Ok(())` at the end of `main()` is equivalent to `return Ok(());`. Notice: no semicolon. Adding a semicolon would turn it into a statement (returning `()` instead of `Result<()>`), which would be a type error.
> [Read more: Functions](https://doc.rust-lang.org/book/ch03-03-how-functions-work.html)

---

## 5. src/tui.rs — Terminal Setup and Teardown

This is the smallest module — just 24 lines — and it handles the low-level terminal mechanics.

```rust
use color_eyre::Result;
use crossterm::{
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io::{self, Stdout};

pub type Tui = Terminal<CrosstermBackend<Stdout>>;
```

**`pub type Tui = Terminal<CrosstermBackend<Stdout>>;`** — This creates a **type alias**. Instead of writing out the full type `Terminal<CrosstermBackend<Stdout>>` everywhere, the code uses `Tui`. It's purely a readability convenience — no new type is created.

> **Rust concept: Generics**
> `Terminal<CrosstermBackend<Stdout>>` uses **generics** — the angle brackets `<>` specify type parameters. `Terminal` is a generic type that works with any backend. Here it's parameterized with `CrosstermBackend`, which itself is parameterized with `Stdout` (standard output). This is like `List<String>` in Java or `Array<number>` in TypeScript.
> [Read more: Generics](https://doc.rust-lang.org/book/ch10-01-syntax.html)

### The init function

```rust
pub fn init() -> Result<Tui> {
    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}
```

Three things happen in sequence:

1. **Raw mode** — Normally, your terminal buffers input line-by-line and echoes typed characters. Raw mode disables all that: every keystroke is delivered immediately, and nothing is echoed. This is essential for a TUI where you need to handle each keypress individually.

2. **Alternate screen** — Terminals have two buffers: the normal one (with your command history) and an alternate one (a blank canvas). Programs like `vim` and `less` use the alternate screen so your shell history is preserved when they exit. `EnterAlternateScreen` switches to this blank canvas.

3. **Terminal object** — ratatui's `Terminal` wraps the backend and provides the `.draw()` method.

### The restore function

```rust
pub fn restore() -> Result<()> {
    terminal::disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;
    Ok(())
}
```

This undoes everything `init` did: turns off raw mode and switches back to the normal screen. If this isn't called (e.g., the program crashes), your terminal would be left in a broken state — you'd need to run `reset` in your shell to fix it.

The `execute!` macro runs crossterm commands immediately on the given writer.

---

## 6. src/event.rs — Keyboard and Tick Events

```rust
use std::time::Duration;
use color_eyre::Result;
use crossterm::event::{self, Event as CrosstermEvent, KeyEvent};
```

**`Event as CrosstermEvent`** — The `as` keyword renames an import to avoid name collisions. Crossterm has its own `Event` type, and this module defines its own `Event` too — so it renames crossterm's to `CrosstermEvent`.

### The Event Enum

```rust
#[derive(Debug)]
pub enum Event {
    Key(KeyEvent),
    Tick,
}
```

This is an enum with **data-carrying variants**. `Key(KeyEvent)` holds a `KeyEvent` value inside it (which key was pressed, with what modifiers). `Tick` carries nothing — it just signals that time has passed with no input.

### The EventHandler

```rust
pub struct EventHandler {
    tick_rate: Duration,
}

impl EventHandler {
    pub fn new(tick_rate_ms: u64) -> Self {
        Self {
            tick_rate: Duration::from_millis(tick_rate_ms),
        }
    }

    pub fn next(&self) -> Result<Event> {
        if event::poll(self.tick_rate)? {
            match event::read()? {
                CrosstermEvent::Key(key) => Ok(Event::Key(key)),
                _ => Ok(Event::Tick),
            }
        } else {
            Ok(Event::Tick)
        }
    }
}
```

> **Rust concept: Structs and `impl`**
> A `struct` defines a data type with named fields — like a class's fields. The `impl` block defines methods on that struct — like a class's methods. Rust separates data from behavior, unlike OOP languages where they're combined in a class.
>
> **`Self`** inside an `impl` block refers to the type being implemented (here, `EventHandler`). `&self` in method signatures means "this method borrows the struct immutably."
> [Read more: Structs](https://doc.rust-lang.org/book/ch05-01-defining-structs.html)
> [Read more: Method syntax](https://doc.rust-lang.org/book/ch05-03-method-syntax.html)

**`pub fn new(tick_rate_ms: u64) -> Self`** — This is a **constructor pattern** in Rust. There's no special `constructor` keyword — by convention, a function called `new` creates a new instance. `u64` is an unsigned 64-bit integer.

**`event::poll(self.tick_rate)?`** — Waits up to `tick_rate` duration for an event. Returns `true` if an event is available, `false` if the timeout expired. The `?` handles potential I/O errors.

**`_ => Ok(Event::Tick)`** — The underscore `_` is a wildcard pattern meaning "match anything else." Mouse events, resize events, etc. are all converted to `Tick` (ignored).

The tick mechanism is important: when no key is pressed for 250ms, the main loop gets a `Tick` event, which triggers `session_list.refresh_states()`. This is how the session state icons update in real time — the hook handler writes state changes to the JSON file, and the tick handler re-reads them.

---

## 7. src/theme.rs — The Color Palette

```rust
use ratatui::style::Color;

// Syndicate-inspired color palette
pub const BG: Color = Color::Rgb(53, 56, 63);        // dark gray background
pub const ORANGE: Color = Color::Rgb(200, 90, 26);    // warm amber/orange — primary accent
pub const ORANGE_BRIGHT: Color = Color::Rgb(220, 120, 40); // brighter orange for focused elements
pub const BLUE: Color = Color::Rgb(74, 106, 138);     // muted steel blue — ghost/secondary
pub const GRAY: Color = Color::Rgb(60, 60, 80);       // mid-gray for unfocused borders
pub const GRAY_DIM: Color = Color::Rgb(65, 137, 181); // muted blue for dimmed text
pub const TEXT: Color = Color::Rgb(180, 180, 195);     // light gray text
pub const ERROR: Color = Color::Rgb(200, 60, 60);     // muted red for errors
pub const SESSION_NAME: Color = Color::Rgb(249, 217, 67); // golden yellow for session names
```

> **Rust concept: Constants**
> `const` defines compile-time constants. They must have an explicit type and their value must be computable at compile time. Unlike `let` bindings, constants can be used at the module level (outside functions) and are inlined wherever they're used.
> [Read more: Constants](https://doc.rust-lang.org/book/ch03-01-variables-and-mutability.html#constants)

`Color::Rgb(r, g, b)` is an enum variant that holds three `u8` values (0-255 each). All colors in this file use 24-bit RGB, meaning the terminal must support true color (most modern terminals do).

These constants are used throughout the rendering code. By centralizing them, changing the look of the entire application means editing only this file.

---

## 8. src/app.rs — The "New Task" Form

This is the largest module (~1,060 lines). It handles a four-field form (title, directory, prompt, CLI args), keyboard navigation, path autocompletion, a recent directories dropdown, validation, and rendering.

### 8.1 Imports

```rust
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Flex, Layout, Position},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use std::path::Path;
use tui_input::Input;
use tui_input::backend::crossterm::EventHandler;

use crate::theme;
```

`crate::theme` means "the `theme` module from the root of this crate." Since `theme` is declared in `main.rs` (the crate root), all other modules access it through `crate::`.

### 8.2 Character Wrapping

```rust
fn char_wrap_lines(text: &str, width: usize) -> Vec<String> {
    if width == 0 || text.is_empty() {
        return vec![text.to_string()];
    }
    let chars: Vec<char> = text.chars().collect();
    chars.chunks(width).map(|c| c.iter().collect()).collect()
}
```

This function breaks text into lines of at most `width` characters. It's used for the prompt input field, which can grow vertically as you type long prompts.

**`.chunks(width)`** — Splits a slice into sub-slices of `width` elements each (the last chunk may be smaller). This is a zero-copy operation on slices.

### 8.3 Directory Tab-Completion

```rust
pub fn complete_path(input: &str) -> Option<(String, Option<String>)> {
    if input.is_empty() {
        return None;
    }

    let path = Path::new(input);

    let (parent, prefix) = if input.ends_with('/') && path.is_dir() {
        (path.to_path_buf(), "")
    } else {
        let parent = path.parent()?;
        let file_name = path.file_name()?.to_str()?;
        (parent.to_path_buf(), file_name)
    };

    let entries = std::fs::read_dir(&parent).ok()?;
    let mut matches: Vec<String> = Vec::new();

    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with(prefix) {
            if entry.path().is_dir() {
                matches.push(name_str.to_string());
            }
        }
    }
    // ...
}
```

This function implements filesystem autocompletion, like pressing Tab in your shell. The return type `Option<(String, Option<String>)>` is a tuple: the completed path string, and optionally a "ghost text" suggestion string.

**`Path::new(input)`** — Creates a `Path` from a string. `Path` is Rust's cross-platform file path type. It doesn't allocate memory — it just borrows the input string. `PathBuf` is the owned version (like `String` vs `&str`).

**`path.parent()?`** — Gets the parent directory. Returns `Option<&Path>`, and the `?` here works on `Option` to return `None` if there's no parent. (Yes, `?` works on both `Result` and `Option` — it short-circuits on the "nothing" case.)

**`.ok()?`** on `read_dir` — `.ok()` converts a `Result` to an `Option` (discarding the error details), then `?` propagates `None`.

**`.flatten()`** — The iterator from `read_dir` yields `Result<DirEntry>` items (each entry might fail). `.flatten()` silently skips the errors, giving you only the successful entries.

**`.to_string_lossy()`** — File names on some operating systems aren't valid UTF-8. This method converts to a Rust `String`, replacing any invalid bytes with the Unicode replacement character. "Lossy" means some data might be lost.

### 8.4 Longest Common Prefix

```rust
fn longest_common_prefix(strings: &[String]) -> String {
    if strings.is_empty() {
        return String::new();
    }
    let first = &strings[0];
    let mut len = first.len();
    for s in &strings[1..] {
        len = len.min(s.len());
        for (i, (a, b)) in first.chars().zip(s.chars()).enumerate() {
            if a != b {
                len = len.min(i);
                break;
            }
        }
    }
    first[..len].to_string()
}
```

**`&[String]`** — A **slice** — a reference to a contiguous sequence of `String` values. Think of it as a view into a `Vec<String>` without owning it.

> **Rust concept: Slices**
> A slice `&[T]` is a "view" into an array or vector. It's a pointer plus a length. It lets you pass around parts of collections without copying. `&str` is actually a string slice — a view into string data.
> [Read more: Slices](https://doc.rust-lang.org/book/ch04-03-slices.html)

**`&strings[1..]`** — Range indexing. `1..` means "from index 1 to the end." This is like Python's `strings[1:]`.

**`.chars().zip(s.chars())`** — `.zip()` pairs up elements from two iterators. If you have `"abc".chars()` and `"abd".chars()`, zip gives you `('a','a'), ('b','b'), ('c','d')`.

**`.enumerate()`** — Adds an index to each item: `(0, ('a','a')), (1, ('b','b')), ...`

### 8.5 The Data Types

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputField {
    Title,
    Directory,
    Prompt,
    ClaudeArgs,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    None,
    Quit,
    Submit {
        title: String,
        directory: String,
        prompt: Option<String>,
        claude_args: Option<String>,
    },
}
```

**`#[derive(...)]`** — The `derive` attribute auto-generates trait implementations:

| Trait | What it provides |
|---|---|
| `Debug` | Printable with `{:?}` format |
| `Clone` | Can be duplicated with `.clone()` |
| `Copy` | Can be duplicated implicitly (only for small, stack-only types) |
| `PartialEq` | Can be compared with `==` |
| `Eq` | Asserts full equality (not just partial — required for some collections) |

> **Rust concept: Traits**
> Traits are Rust's version of interfaces. They define shared behavior. `PartialEq` is a trait that requires implementing `fn eq(&self, other: &Self) -> bool`. `derive` auto-generates this implementation by comparing each field.
> [Read more: Traits](https://doc.rust-lang.org/book/ch10-02-traits.html)

Notice `InputField` derives `Copy` but `Action` does not. `Copy` can only be derived for types where all fields are `Copy` — `String` isn't `Copy` (it owns heap memory), so `Action` can't be either.

The `InputField` enum now has four variants — the original three plus `ClaudeArgs` for passing additional CLI arguments to Claude. The `Action::Submit` variant also gained a `claude_args` field.

### 8.6 The App Struct

```rust
#[derive(Debug)]
pub struct App {
    pub running: bool,
    pub focused_field: InputField,
    pub title_input: Input,
    pub dir_input: Input,
    pub prompt_input: Input,
    pub claude_args_input: Input,
    pub dir_suggestion: Option<String>,
    pub error_message: Option<String>,
    pub recent_dirs: Vec<String>,
    pub recent_dir_selected: Option<usize>,
    pub show_recent_dirs: bool,
}
```

This struct holds all state for the "New Task" form. `Input` is from the `tui-input` crate — a stateful text input widget.

The struct has grown from the original: `claude_args_input` is a fourth input field, and `recent_dirs`, `recent_dir_selected`, and `show_recent_dirs` manage the recent directories dropdown overlay.

### 8.7 The Constructor

```rust
impl App {
    #[cfg(test)]
    pub fn new() -> Self {
        Self::with_recent_dirs(Vec::new())
    }

    pub fn with_recent_dirs(recent_dirs: Vec<String>) -> Self {
        let default_dir = recent_dirs.first().cloned().unwrap_or_else(|| {
            std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default()
        });

        Self {
            running: true,
            focused_field: InputField::Title,
            title_input: Input::default(),
            dir_input: Input::new(default_dir),
            prompt_input: Input::default(),
            claude_args_input: Input::default(),
            dir_suggestion: None,
            error_message: None,
            recent_dirs,
            recent_dir_selected: None,
            show_recent_dirs: false,
        }
    }
```

**`#[cfg(test)]`** on `new()` — This constructor only exists in test builds. In production, `with_recent_dirs()` is used instead, which accepts the list of recent directories.

**`recent_dirs.first().cloned().unwrap_or_else(|| ...)`** — The default directory is the most recent directory if available, otherwise the current working directory. `.first()` returns `Option<&String>`, `.cloned()` converts to `Option<String>` (since we need ownership), and `.unwrap_or_else()` provides a fallback computed lazily.

**`Input::default()`** — The `Default` trait provides a "zero value" constructor. For `Input`, this means an empty text field.

### 8.8 Keyboard Handling

```rust
pub fn handle_key(&mut self, key: KeyEvent) -> Action {
    // Handle recent dirs dropdown navigation
    if self.show_recent_dirs {
        return self.handle_recent_dirs_key(key);
    }

    match key.code {
        KeyCode::Esc => {
            self.quit();
            Action::Quit
        }
        KeyCode::Tab | KeyCode::Down => {
            self.next_field();
            Action::None
        }
        KeyCode::BackTab | KeyCode::Up => {
            self.prev_field();
            Action::None
        }
        KeyCode::Right
            if self.focused_field == InputField::Directory
                && self.dir_suggestion.is_some()
                && self.cursor_at_end() =>
        {
            self.complete_directory();
            Action::None
        }
        KeyCode::Enter => self.handle_enter(),
        _ => {
            // Ctrl+D toggles recent dirs when on directory field
            if self.focused_field == InputField::Directory
                && key.code == KeyCode::Char('d')
                && key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL)
                && !self.recent_dirs.is_empty()
            {
                self.show_recent_dirs = true;
                self.recent_dir_selected = Some(0);
                return Action::None;
            }

            match self.focused_field {
                InputField::Title => {
                    self.title_input
                        .handle_event(&crossterm::event::Event::Key(key));
                }
                // ... (similar for other fields)
            }
            self.error_message = None;
            Action::None
        }
    }
}
```

**`&mut self`** — This method borrows `App` mutably, meaning it can modify the struct's fields. If it used `&self` instead, the compiler would reject any field assignments.

The `match` on `KeyCode::Right` has a **guard clause**: `if self.focused_field == InputField::Directory && ...`. Guards add extra conditions to match arms — the pattern matches only if both the pattern and the guard are true.

**`Ctrl+D`** — When pressed on the directory field with recent directories available, it opens the dropdown. This is checked in the catch-all `_ =>` arm using `key.modifiers.contains()`.

The field cycle now includes four fields: Title → Directory → Prompt → ClaudeArgs → Title.

### 8.9 Recent Directories Dropdown

```rust
fn handle_recent_dirs_key(&mut self, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc => {
            self.show_recent_dirs = false;
            self.recent_dir_selected = None;
            Action::None
        }
        KeyCode::Up | KeyCode::BackTab => {
            if let Some(i) = self.recent_dir_selected {
                if i > 0 {
                    self.recent_dir_selected = Some(i - 1);
                } else {
                    self.recent_dir_selected = Some(self.recent_dirs.len() - 1);
                }
            }
            Action::None
        }
        KeyCode::Down | KeyCode::Tab => {
            // ... wrapping navigation
        }
        KeyCode::Enter => {
            if let Some(i) = self.recent_dir_selected
                && let Some(dir) = self.recent_dirs.get(i)
            {
                let dir = dir.clone();
                self.dir_input = Input::new(dir.clone());
                // Move cursor to end ...
            }
            self.show_recent_dirs = false;
            self.recent_dir_selected = None;
            Action::None
        }
        _ => Action::None,
    }
}
```

When the dropdown is open, all key events are intercepted by this handler. Navigation wraps around (Down at the bottom goes to top, Up at the top goes to bottom). Enter selects the highlighted directory and populates the directory input field.

**`if let Some(i) = ... && let Some(dir) = ...`** — This is a **let chain** (stabilized in Rust 2024 edition). It combines two `if let` conditions — both must succeed for the block to execute.

### 8.10 Form Validation

```rust
fn handle_enter(&mut self) -> Action {
    let title = self.title_input.value().trim().to_string();
    let directory = self.dir_input.value().trim().to_string();

    if title.is_empty() {
        self.error_message = Some("Title cannot be empty".to_string());
        self.focused_field = InputField::Title;
        return Action::None;
    }

    if directory.is_empty() {
        self.error_message = Some("Directory cannot be empty".to_string());
        return Action::None;
    }

    if !Path::new(&directory).is_dir() {
        self.error_message = Some(format!("Directory does not exist: {directory}"));
        return Action::None;
    }

    let prompt_raw = self.prompt_input.value().trim().to_string();
    let prompt = if prompt_raw.is_empty() { None } else { Some(prompt_raw) };

    let args_raw = self.claude_args_input.value().trim().to_string();
    let claude_args = if args_raw.is_empty() { None } else { Some(args_raw) };

    Action::Submit { title, directory, prompt, claude_args }
}
```

Enter submits from any field — it always validates all fields at once. Empty optional fields (prompt, claude_args) are converted to `None`.

**`.trim().to_string()`** — `.trim()` returns a `&str` (a slice with leading/trailing whitespace removed). `.to_string()` creates an owned `String` from it. This is needed because the `Action::Submit` variant owns its `String` fields.

> **Rust concept: String vs &str**
> Rust has two main string types:
> - `String` — An owned, heap-allocated, growable string. You can modify it, store it in structs, return it from functions.
> - `&str` — A borrowed reference to string data. Lightweight, but you can't store it without a lifetime annotation.
>
> Most functions take `&str` as input (flexible — accepts both types) and return `String` when they need to give ownership to the caller.
> [Read more: Strings](https://doc.rust-lang.org/book/ch08-02-strings.html)

**`format!("Directory does not exist: {directory}")`** — The `format!` macro creates a `String` with interpolated values. The `{directory}` syntax embeds the variable directly (Rust 2021+ feature, similar to JavaScript template literals).

### 8.11 Drawing the Form

```rust
pub fn draw(&self, frame: &mut Frame) {
    let area = frame.area();

    let form_width = 90u16.min(area.width.saturating_sub(2));

    // Calculate prompt input height based on text wrapping
    let prompt_inner_width = form_width.saturating_sub(4) as usize;
    let prompt_lines = if prompt_inner_width == 0 {
        1
    } else {
        let text_len = self.prompt_input.value().len();
        ((text_len as f64 / prompt_inner_width as f64).ceil() as u16).max(1)
    };
    let max_prompt_height = area.height.saturating_sub(16);
    let prompt_box_height = (prompt_lines + 2).min(max_prompt_height).max(3);

    let form_height = (16 + prompt_box_height).min(area.height.saturating_sub(2));
    let total_height = form_height + 1; // +1 for error line below the box
```

**`90u16`** — The form is 90 columns wide (up from the original 60), accommodating longer prompts and directory paths. The `u16` suffix specifies the type of the literal. `u16` is an unsigned 16-bit integer. ratatui uses `u16` for screen coordinates because terminal dimensions never exceed 65,535.

**`.saturating_sub(2)`** — Subtraction that stops at zero instead of overflowing. Since `u16` is unsigned, `0 - 2` would panic or wrap around. Saturating subtraction prevents this: `1.saturating_sub(2)` gives `0`.

The prompt field dynamically grows in height as you type — the form calculates how many wrapped lines the current text occupies and adjusts the layout accordingly.

**Layout and Constraint** — ratatui's layout system works by splitting a rectangular area into chunks:
- `Layout::vertical([...])` splits vertically
- `Layout::horizontal([...])` splits horizontally
- `Constraint::Length(n)` requests exactly `n` cells
- `Flex::Center` centers the content within the available space

The inner form layout:

```rust
let chunks = Layout::vertical([
    Constraint::Length(1),                 // Title label
    Constraint::Length(3),                 // Title input
    Constraint::Length(1),                 // Directory label
    Constraint::Length(3),                 // Directory input
    Constraint::Length(1),                 // Prompt label
    Constraint::Length(prompt_box_height), // Prompt input (grows with text)
    Constraint::Length(1),                 // Claude args label
    Constraint::Length(3),                 // Claude args input
    Constraint::Min(1),                    // Hints
])
.split(inner);
```

`Constraint::Min(1)` means "at least 1 row, but take up any remaining space."

The ghost-text autocompletion for the directory field uses `Span` (a styled chunk of text) and `Line` (a sequence of spans):

```rust
let dir_line = if let Some(ref suggestion) = self.dir_suggestion {
    Line::from(vec![
        Span::styled(dir_value.to_string(), Style::default().fg(theme::TEXT)),
        Span::styled(suggestion.as_str(), Style::default().fg(theme::BLUE)),
    ])
} else {
    Line::from(Span::styled(
        dir_value.to_string(),
        Style::default().fg(theme::TEXT),
    ))
};
```

**`if let Some(ref suggestion) = self.dir_suggestion`** — This is a conditional pattern match. It's like `match` but for a single pattern. If `dir_suggestion` is `Some`, the inner value is bound to `suggestion`. The `ref` keyword borrows the value rather than moving it.

> **Rust concept: `if let`**
> `if let` is sugar for a `match` with two arms where you only care about one case.
> [Read more: if let](https://doc.rust-lang.org/book/ch06-03-if-let.html)

### 8.12 The Recent Directories Dropdown Overlay

```rust
if self.show_recent_dirs && !self.recent_dirs.is_empty() {
    let dropdown_height = self.recent_dirs.len() as u16 + 2;
    let dropdown_area = ratatui::layout::Rect {
        x: chunks[3].x,
        y: chunks[3].y + chunks[3].height,
        width: chunks[3].width,
        height: dropdown_height.min(7),
    };
    frame.render_widget(Clear, dropdown_area);
    // ... render list items with selection highlighting
}
```

The dropdown is positioned directly below the directory input field and renders over whatever content is underneath (using `Clear` to erase the background first). The selected item is highlighted with the `GRAY` background color, while unselected items use `GRAY_DIM` text.

**`Rect { x, y, width, height }`** — When you need a custom area that isn't produced by `Layout`, you can construct a `Rect` directly. Here the dropdown is anchored to the directory input's position.

---

## 9. src/session_list.rs — The Session Browser

### 9.1 Actions and State

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionListAction {
    None,
    Quit,
    NewTask,
    Attach { session_name: String },
}

#[derive(Debug)]
pub struct SessionList {
    pub sessions: Vec<SessionRecord>,
    pub list_state: ListState,
    pub status_message: Option<String>,
}
```

`ListState` is a ratatui type that tracks which item in a list is currently selected. It stores an `Option<usize>` — `None` means nothing is selected.

### 9.2 Refreshing from Disk

```rust
pub fn refresh(&mut self) {
    match crate::session::list_sessions() {
        Ok(sessions) => {
            let alive: Vec<SessionRecord> = sessions
                .into_iter()
                .filter(|s| tmux::session_exists(&s.tmux_session_name).unwrap_or(false))
                .collect();
            self.sessions = alive;
            self.clamp_selection();
        }
        Err(e) => {
            self.status_message = Some(format!("Error loading sessions: {e}"));
        }
    }
}
```

The full refresh checks tmux to see which sessions are still alive. This is used after creating, killing, or attaching to sessions.

### 9.3 Lightweight State Refresh

```rust
pub fn refresh_states(&mut self) {
    let Ok(db_sessions) = crate::session::list_sessions() else {
        return;
    };
    for session in &mut self.sessions {
        if let Some(updated) = db_sessions
            .iter()
            .find(|s| s.tmux_session_name == session.tmux_session_name)
        {
            session.state = updated.state.clone();
        }
    }
}
```

Unlike the full refresh, this only re-reads session states from the database without spawning tmux processes. It's called on every tick (every 250ms) and is designed to be fast.

**`let Ok(db_sessions) = ... else { return; }`** — This is a **let-else** statement (stabilized in Rust 1.65). It's like `if let` but for the failure path: if the pattern doesn't match, the `else` block must diverge (here, `return`). If it does match, the binding is available for the rest of the function.

**`for session in &mut self.sessions`** — Iterates with mutable references, allowing each `session.state` to be updated in place.

### 9.4 Selection Helpers

```rust
pub fn select_by_name(&mut self, name: &str) {
    let idx = self
        .sessions
        .iter()
        .position(|s| s.tmux_session_name == name)
        .unwrap_or(0);
    if !self.sessions.is_empty() {
        self.list_state.select(Some(idx));
    }
}

fn clamp_selection(&mut self) {
    if self.sessions.is_empty() {
        self.list_state.select(None);
    } else if self.list_state.selected().is_none() {
        self.list_state.select(Some(0));
    } else if let Some(i) = self.list_state.selected()
        && i >= self.sessions.len()
    {
        self.list_state.select(Some(self.sessions.len() - 1));
    }
}
```

`select_by_name` is used after creating a new session so the list jumps to the newly created entry. `clamp_selection` ensures the selection stays valid after the list changes (e.g., if a session is killed and the list shrinks).

The `if let Some(i) = ... && i >= ...` syntax is a **let chain** (stabilized in Rust 2024 edition). It reads: "if there's a selected index `i`, AND that index is past the end of the list, then clamp it to the last valid index."

### 9.5 Wrapping Navigation

```rust
fn move_down(&mut self) {
    if self.sessions.is_empty() {
        return;
    }
    let i = match self.list_state.selected() {
        Some(i) => {
            if i >= self.sessions.len() - 1 {
                0
            } else {
                i + 1
            }
        }
        None => 0,
    };
    self.list_state.select(Some(i));
}
```

When you press `j` or Down at the bottom of the list, it wraps back to the top (index 0). When you press `k` or Up at the top, it wraps to the bottom. This is standard vim-style navigation.

### 9.6 Killing a Session

```rust
fn kill_selected(&mut self) {
    if let Some(i) = self.list_state.selected() {
        let session = &self.sessions[i];
        let name = session.tmux_session_name.clone();
        let dir = session.directory.clone();
        match tmux::kill_session(&name) {
            Ok(()) => {
                if let Err(e) = tmux::remove_worktree(&dir, &name) {
                    self.status_message =
                        Some(format!("Killed session but failed to remove worktree: {e}"));
                } else {
                    self.status_message = Some(format!("Killed session: {name}"));
                }
                let _ = crate::session::remove_session(&name);
                self.refresh();
            }
            Err(e) => {
                self.status_message = Some(format!("Failed to kill '{name}': {e}"));
            }
        }
    }
}
```

Killing a session now does three things: kills the tmux session, removes the worktree directory (the git worktree Claude was using), and removes the record from the JSON database.

**`.clone()`** — Creates a deep copy of the `String`. This is needed because `session` is borrowed from `self.sessions`, and we need the name to outlive that borrow (since `self.refresh()` will mutate `self.sessions`).

**`let _ = crate::session::remove_session(&name);`** — The `let _ =` pattern deliberately discards the result. The underscore tells the compiler (and the reader) "I know this returns a `Result`, and I'm intentionally ignoring it." This is used here because if removing from the JSON file fails, the tmux session is already dead — there's no useful recovery.

### 9.7 Rendering the Session List

```rust
let items: Vec<ListItem> = self
    .sessions
    .iter()
    .map(|s| {
        let icon_color = match s.state {
            SessionState::Working => theme::ORANGE_BRIGHT,
            SessionState::WaitingUser => Color::Yellow,
            SessionState::Idle => theme::GRAY_DIM,
        };
        let line = Line::from(vec![
            Span::styled(s.state.icon(), Style::default().fg(icon_color)),
            Span::raw(" "),
            Span::styled(
                &s.tmux_session_name,
                Style::default()
                    .fg(theme::SESSION_NAME)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(&s.directory, Style::default().fg(theme::GRAY_DIM)),
        ]);
        ListItem::new(line)
    })
    .collect();

let list = List::new(items)
    .highlight_style(
        Style::default()
            .bg(theme::GRAY)
            .add_modifier(Modifier::BOLD),
    )
    .highlight_symbol("▸ ");

frame.render_stateful_widget(list, chunks[0], &mut self.list_state);
```

Each session displays a state icon, its name in golden yellow, and its directory path in dimmed blue. The state icon is color-coded:

| State | Icon | Color |
|---|---|---|
| Working | ⚙ | Bright orange |
| WaitingUser | ⏳ | Yellow |
| Idle | ● | Dimmed blue |

**`.iter()`** — Creates an iterator that borrows each element (unlike `.into_iter()` which would consume the vector). This is important because we need `self.sessions` to survive — we're just reading from it.

**`.map(|s| { ... })`** — Transforms each `SessionRecord` into a `ListItem` widget.

**`render_stateful_widget`** — Unlike `render_widget`, this takes a mutable reference to state (`&mut self.list_state`). ratatui uses this to track scroll position and selection state across frames.

The `"▸ "` marker appears next to the currently selected session.

---

## 10. src/tmux.rs — Talking to tmux

This module shells out to the `tmux` command-line tool.

### 10.1 The Session Struct

```rust
#[derive(Debug)]
pub struct TmuxSession {
    pub session_id: String,
}
```

A simple data holder returned after creating a session. `session_id` is tmux's internal ID like `"$1"`, `"$2"`, etc.

### 10.2 Sanitizing Session Names

```rust
pub fn sanitize_session_name(title: &str) -> String {
    let lowered = title.to_lowercase();
    let mut result = String::new();
    let mut prev_was_sep = true;

    for ch in lowered.chars() {
        if ch.is_whitespace() || ch == '-' {
            if !prev_was_sep {
                result.push('-');
                prev_was_sep = true;
            }
        } else if ch.is_alphanumeric() {
            result.push(ch);
            prev_was_sep = false;
        }
    }

    while result.ends_with('-') {
        result.pop();
    }

    result
}
```

tmux session names can't contain dots, colons, or most special characters. This function slugifies a title: `"My Cool Task!"` becomes `"my-cool-task"`.

The `prev_was_sep` flag collapses consecutive separators into a single hyphen. Starting with `true` also trims leading hyphens.

**`.push('-')`** — Appends a character to a `String`. Strings in Rust are UTF-8 encoded and heap-allocated, growing as needed.

### 10.3 Creating a Session

```rust
pub fn create_session(
    name: &str,
    dir: &str,
    prompt: Option<&str>,
    claude_args: Option<&str>,
    claude_session_id: &str,
) -> Result<TmuxSession> {
    let abs_dir = std::path::Path::new(dir)
        .canonicalize()
        .map_err(|e| eyre!("Cannot resolve directory '{dir}': {e}"))?
        .to_string_lossy()
        .to_string();

    let mut claude_cmd = format!("claude --worktree {name} --session-id {claude_session_id}");
    if let Some(args) = claude_args {
        claude_cmd.push(' ');
        claude_cmd.push_str(args);
    }
    if let Some(p) = prompt {
        claude_cmd.push(' ');
        claude_cmd.push_str(&shell_escape(p));
    }

    let status = Command::new("tmux")
        .args([
            "new-session", "-d", "-s", name,
            "-n", "claude",
            "-c", &abs_dir,
            &claude_cmd,
        ])
        .status()?;
    if !status.success() {
        return Err(eyre!("Failed to create tmux session '{name}'"));
    }

    let output = Command::new("tmux")
        .args(["display-message", "-t", name, "-p", "#{session_id}"])
        .output()?;
    // ... parse session_id
```

**`Command::new("tmux")`** — Rust's standard library provides `std::process::Command` for spawning child processes. It's a builder pattern:
- `.args([...])` — Adds arguments
- `.status()` — Runs the command, waits for it to finish, and returns its exit status

The function now takes five parameters: session name, directory, optional prompt, optional additional CLI args, and the pre-generated Claude session ID. The Claude command includes `--worktree` (for isolated git worktrees) and `--session-id` (to correlate hook events).

**`.canonicalize()`** — Resolves the directory path to an absolute, symlink-free path. This ensures tmux gets a clean path regardless of how the user typed it.

Note that `create_session` now only creates the claude window — it no longer creates the editor window. The editor window setup is deferred to the `SessionStart` hook (see process_hook module), which fires after Claude initializes and creates its worktree.

### 10.4 Setting Up the Editor Window

```rust
pub fn setup_editor_window(session_name: &str, directory: &str) -> Result<()> {
    let abs_dir = std::path::Path::new(directory)
        .canonicalize()
        .map_err(|e| eyre!("Cannot resolve directory '{directory}': {e}"))?
        .to_string_lossy()
        .to_string();

    let worktree_dir = format!("{abs_dir}/.claude/worktrees/{session_name}");
    let worktree_path = std::path::Path::new(&worktree_dir);

    let editor_dir = if worktree_path.exists() {
        &worktree_dir
    } else {
        &abs_dir
    };

    // Split claude window horizontally to add a terminal pane
    run_tmux(&["split-window", "-h", "-t", &format!("{session_name}:claude"), "-c", editor_dir])?;
    // Keep focus on the claude pane (left)
    run_tmux(&["select-pane", "-t", &format!("{session_name}:claude.0")])?;
    // Create editor window with vim
    run_tmux(&["new-window", "-t", session_name, "-n", "editor", "-c", editor_dir])?;
    run_tmux(&["send-keys", "-t", &format!("{session_name}:editor"), "vim .", "Enter"])?;
    run_tmux(&["split-window", "-h", "-t", &format!("{session_name}:editor"), "-c", editor_dir])?;
    // Select the claude window as the default
    run_tmux(&["select-window", "-t", &format!("{session_name}:claude")])?;

    Ok(())
}
```

This is called by the `process-hook` handler when Claude's `SessionStart` event fires. At that point, Claude has already created its git worktree at `.claude/worktrees/<session-name>`, so the editor and terminal panes open directly in the worktree. If the worktree doesn't exist (fallback), it uses the project directory.

The resulting tmux session has two windows:
- **claude** — Claude on the left pane, a terminal on the right pane (both in the worktree)
- **editor** — vim on the left pane, a shell on the right pane (both in the worktree)

### 10.5 Switching and Killing Sessions

```rust
pub fn switch_to_session(name: &str) -> Result<()> {
    run_tmux(&["switch-client", "-t", name])
}

pub fn kill_session(name: &str) -> Result<()> {
    run_tmux(&["kill-session", "-t", name])
}
```

`switch_to_session` uses `switch-client` instead of `attach-session` because the TUI is running inside tmux. `switch-client` changes which session the current tmux client is viewing, while `attach-session` would try to create a new client.

### 10.6 Worktree Cleanup

```rust
pub fn remove_worktree(dir: &str, name: &str) -> Result<()> {
    let worktree_dir = std::path::PathBuf::from(dir)
        .join(".claude")
        .join("worktrees")
        .join(name);
    if worktree_dir.exists() {
        std::fs::remove_dir_all(&worktree_dir)?;
    }
    Ok(())
}
```

When a session is killed, its worktree directory at `<project>/.claude/worktrees/<session-name>` is cleaned up. `remove_dir_all` recursively deletes the directory and all its contents.

**`PathBuf::from(dir).join(".claude").join("worktrees").join(name)`** — `PathBuf` methods chain nicely for building paths. Each `.join()` appends a path component with the correct separator for the OS.

### 10.7 Shell Escaping

```rust
fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
```

Wraps a string in single quotes for safe shell embedding. The trick `'\''` ends the current single-quoted string, adds an escaped single quote, and starts a new single-quoted string. This is the standard POSIX way to include a literal `'` inside a single-quoted string.

### 10.8 The Helper Function

```rust
fn run_tmux(args: &[&str]) -> Result<()> {
    let status = Command::new("tmux").args(args).status()?;
    if !status.success() {
        return Err(eyre!("tmux command failed: tmux {}", args.join(" ")));
    }
    Ok(())
}
```

**`&[&str]`** — A slice of string slices. This is Rust's way of accepting a variable-length list of string arguments. The function doesn't own any of the strings — it just borrows them long enough to pass to tmux.

---

## 11. src/session.rs — Persisting Data to JSON

### 11.1 Session State

```rust
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum SessionState {
    Working,
    WaitingUser,
    Idle,
}

impl SessionState {
    pub fn icon(&self) -> &'static str {
        match self {
            SessionState::Working => "⚙",
            SessionState::WaitingUser => "⏳",
            SessionState::Idle => "●",
        }
    }
}

impl std::fmt::Display for SessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionState::Working => write!(f, "Working"),
            SessionState::WaitingUser => write!(f, "Waiting User"),
            SessionState::Idle => write!(f, "Idle"),
        }
    }
}
```

`SessionState` is a new enum that tracks what Claude is doing in each session. It implements two traits beyond the derived ones:

- **`icon()`** — Returns the Unicode symbol displayed in the session list
- **`Display`** — The standard trait for human-readable formatting. Implementing this lets you use `{}` in format strings (as opposed to `{:?}` which uses `Debug`).

**`&'static str`** — A string slice with a `'static` lifetime, meaning it lives for the entire program. String literals like `"⚙"` are always `'static` because they're embedded in the binary.

> **Rust concept: Lifetimes**
> Lifetimes are Rust's way of ensuring references don't outlive the data they point to. `'static` is the longest possible lifetime — the data exists for the whole program. You'll rarely need to write explicit lifetimes in application code, but you'll see `'static` on string literals.
> [Read more: Lifetimes](https://doc.rust-lang.org/book/ch10-03-lifetime-syntax.html)

### 11.2 Data Structures with Serde

```rust
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SessionRecord {
    pub tmux_session_id: String,
    pub tmux_session_name: String,
    pub claude_session_id: Option<String>,
    pub directory: String,
    pub created_at: u64,
    #[serde(default = "default_state")]
    pub state: SessionState,
}

fn default_state() -> SessionState {
    SessionState::Idle
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct SessionDb {
    pub sessions: Vec<SessionRecord>,
}
```

> **Rust concept: Serde (Serialize + Deserialize)**
> `Serialize` and `Deserialize` are traits from the serde crate. When you derive them, the compiler auto-generates code to convert your struct to/from formats like JSON, TOML, YAML, etc.
>
> For this struct, deriving `Serialize` means you can write `serde_json::to_string(&record)` and get a JSON string. Deriving `Deserialize` means `serde_json::from_str::<SessionRecord>(&json)` parses a JSON string back into the struct.
>
> The field names in the struct become the JSON keys. `Option<String>` fields serialize to `null` when `None`.

**`#[serde(default = "default_state")]`** — This serde attribute tells the deserializer: "if the `state` field is missing from the JSON, call the `default_state()` function to get a value." This provides backward compatibility — old session records that were created before `state` was added will deserialize with `Idle` as the default state.

**`Default`** on `SessionDb` — The `Default` trait provides a default value. For `SessionDb`, the default is an empty `sessions` vector.

The `SessionRecord` has two new fields compared to the original:
- **`claude_session_id`** — A UUID that correlates this session with Claude Code's hook events
- **`state`** — Tracks what Claude is doing (Working, WaitingUser, Idle)

### 11.3 Loading and Saving

```rust
fn load_db_from(path: &Path) -> Result<SessionDb> {
    if !path.exists() {
        return Ok(SessionDb::default());
    }
    let contents = fs::read_to_string(path)?;
    let db: SessionDb = serde_json::from_str(&contents)?;
    Ok(db)
}

fn save_db_to(path: &Path, db: &SessionDb) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(db)?;
    fs::write(path, json)?;
    Ok(())
}
```

**`fs::read_to_string(path)?`** — Reads an entire file into a `String`. The `?` propagates I/O errors (file not found, permission denied, etc.).

**`serde_json::from_str(&contents)?`** — Parses JSON into a `SessionDb`. If the JSON is malformed or doesn't match the struct's shape, this returns an error.

**`serde_json::to_string_pretty(db)?`** — Serializes the database to indented JSON. `to_string` would produce compact single-line JSON.

**`fs::create_dir_all(parent)?`** — Recursively creates directories (like `mkdir -p`). This ensures `~/.van-damme/` exists before writing.

### 11.4 Adding a Session

```rust
fn add_session_to(
    path: &Path,
    tmux_session_id: String,
    tmux_session_name: String,
    claude_session_id: String,
    directory: String,
) -> Result<SessionRecord> {
    let mut db = load_db_from(path)?;
    let created_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let record = SessionRecord {
        tmux_session_id,
        tmux_session_name,
        claude_session_id: Some(claude_session_id),
        directory,
        created_at,
        state: SessionState::Idle,
    };

    db.sessions.push(record.clone());
    save_db_to(path, &db)?;
    Ok(record)
}
```

**`SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()`** — Gets the current Unix timestamp (seconds since January 1, 1970). The `.unwrap()` is safe here because the system clock can only be before UNIX_EPOCH if something is seriously wrong.

The function now takes `claude_session_id` as a required parameter (wrapped in `Some` for the record). New sessions always start in the `Idle` state.

Notice the function parameters `tmux_session_id: String` take ownership (no `&`). The caller gives up these strings, and they become part of the `SessionRecord`. This is an intentional design choice: the record needs to own its data for long-term storage.

> **Rust concept: Ownership transfer in function parameters**
> When a function takes `String` (not `&String` or `&str`), the caller's value is **moved** into the function. The caller can no longer use it after the call. This is how Rust ensures there's always exactly one owner of heap data.
> [Read more: Ownership and Functions](https://doc.rust-lang.org/book/ch04-01-what-is-ownership.html#ownership-and-functions)

### 11.5 Updating Session State

```rust
fn update_state_by_claude_session_at(
    path: &Path,
    claude_session_id: &str,
    state: SessionState,
) -> Result<()> {
    let mut db = load_db_from(path)?;
    let session = db
        .sessions
        .iter_mut()
        .find(|s| s.claude_session_id.as_deref() == Some(claude_session_id))
        .ok_or_else(|| eyre!("No session with claude_session_id '{}'", claude_session_id))?;
    session.state = state;
    save_db_to(path, &db)?;
    Ok(())
}
```

This function looks up a session by its Claude session ID and updates its state. It's called by the hook handler when Claude transitions between states.

**`.iter_mut()`** — Returns mutable references, allowing the found session to be modified in place.

**`.as_deref()`** — Converts `Option<String>` to `Option<&str>`, which can then be compared with `Some(claude_session_id)`. Without this, you'd be comparing `Option<&String>` with `Option<&str>`, which wouldn't type-check.

**`.ok_or_else(|| eyre!(...))?`** — Converts `Option` to `Result`. If `find` returns `None`, this creates an error. If it returns `Some`, you get the inner value. Combined with `?`, this is the idiomatic way to "require" that an `Option` has a value.

### 11.6 Updating the tmux Session ID

```rust
pub fn update_tmux_session_id(tmux_session_name: &str, tmux_session_id: &str) -> Result<()> {
    // ... loads DB, finds by name, updates tmux_session_id, saves
}
```

This fills in the real tmux session ID (like `"$42"`) after the placeholder was created. It looks up the session by tmux name (not Claude session ID).

### 11.7 Finding by Claude Session ID

```rust
pub fn find_by_claude_session(claude_session_id: &str) -> Result<Option<SessionRecord>> {
    let path = default_db_path()?;
    let db = load_db_from(&path)?;
    Ok(db
        .sessions
        .into_iter()
        .find(|s| s.claude_session_id.as_deref() == Some(claude_session_id)))
}
```

This is used by the hook handler to look up the session record when Claude fires a `SessionStart` event — it needs the tmux session name and directory to set up the editor window.

### 11.8 Removing a Session

```rust
fn remove_session_from(path: &Path, tmux_session_name: &str) -> Result<()> {
    let mut db = load_db_from(path)?;
    db.sessions
        .retain(|s| s.tmux_session_name != tmux_session_name);
    save_db_to(path, &db)?;
    Ok(())
}
```

**`.retain(|s| ...)`** — Keeps only elements where the closure returns `true`. This is like `.filter()` but operates in-place on the vector, without creating a new one.

### 11.9 The Public/Private Split

Notice the pattern throughout this module:

```rust
pub fn list_sessions() -> Result<Vec<SessionRecord>> {
    let path = default_db_path()?;
    list_sessions_from(&path)
}

fn list_sessions_from(path: &Path) -> Result<Vec<SessionRecord>> {
    let db = load_db_from(path)?;
    Ok(db.sessions)
}
```

Each operation has a public version (uses the default path) and a private `_from` / `_at` version (takes a path parameter). The public version is the API that the rest of the application uses. The private version is used in tests, where a temporary file path is injected instead of the real `~/.van-damme/sessions.json`. This is **dependency injection** through function parameters.

> **Rust concept: Visibility**
> `pub` makes an item public (accessible from outside the module). Without `pub`, items are private to their module. In this module, `list_sessions` is public, but `list_sessions_from` is private — only test code within this same module can call it.
> [Read more: Visibility](https://doc.rust-lang.org/book/ch07-03-paths-for-referring-to-an-item-in-the-module-tree.html)

---

## 12. src/process_hook.rs — Claude Code Hooks

This module handles Claude Code's hook system — a way for Claude to notify external programs about state changes.

### 12.1 The Hook Event

```rust
use crate::session::{self, SessionState};
use crate::tmux;
use color_eyre::Result;
use serde::Deserialize;
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};

#[derive(Deserialize)]
struct HookEvent {
    session_id: String,
    hook_event_name: String,
}
```

Claude Code sends JSON events to hooks via stdin. The struct only extracts the two fields we need — serde will ignore any extra fields in the JSON.

### 12.2 Mapping Events to States

```rust
fn state_for_event(event_name: &str) -> Option<SessionState> {
    match event_name {
        "Stop" => Some(SessionState::Idle),
        "UserPromptSubmit" => Some(SessionState::Working),
        "PermissionRequest" => Some(SessionState::WaitingUser),
        _ => None,
    }
}
```

Three Claude Code events map to session states:
- **Stop** — Claude finished working → Idle
- **UserPromptSubmit** — User submitted a prompt → Working
- **PermissionRequest** — Claude needs permission to do something → WaitingUser

Other events (like `SessionStart`) return `None` — they don't map to a state but may be handled separately.

### 12.3 The Main Handler

```rust
pub fn run() -> Result<()> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;

    log_input(&input)?;

    let event: HookEvent = serde_json::from_str(&input)?;

    if let Some(state) = state_for_event(&event.hook_event_name) {
        let _ = session::update_state_by_claude_session(&event.session_id, state);
    }

    if event.hook_event_name == "SessionStart"
        && let Ok(Some(record)) = session::find_by_claude_session(&event.session_id)
    {
        let _ = tmux::setup_editor_window(&record.tmux_session_name, &record.directory);
    }

    Ok(())
}
```

The handler:

1. **Reads all stdin** into a string (the hook event JSON)
2. **Logs it** to `~/.van-damme/debug.log` for debugging
3. **Parses** the JSON into a `HookEvent`
4. **Updates session state** if the event maps to a state change
5. **Sets up the editor window** on `SessionStart` — this is the deferred editor setup triggered after Claude creates its worktree

The `let _ =` on state updates means: "silently ignore if this session isn't tracked by us." A Claude session started outside van-damme would fire hooks too, and we don't want those to crash the handler.

The `SessionStart` handling uses a let chain: `if event_name == "SessionStart" && let Ok(Some(record)) = ...`. This only proceeds if both the event name matches AND the session lookup succeeds AND returns Some.

### 12.4 Debug Logging

```rust
fn log_input(input: &str) -> Result<()> {
    let parsed: serde_json::Value = serde_json::from_str(input)?;
    let pretty = serde_json::to_string_pretty(&parsed)?;

    let log_path = dirs::home_dir()
        .ok_or_else(|| color_eyre::eyre::eyre!("Could not determine home directory"))?
        .join(".van-damme")
        .join("debug.log");

    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;

    writeln!(file, "{pretty}")?;
    Ok(())
}
```

**`serde_json::Value`** — A dynamic JSON type (like JavaScript's plain objects). By parsing into `Value` first and re-serializing with `to_string_pretty`, the log file gets nicely formatted JSON regardless of the input format.

**`OpenOptions::new().create(true).append(true)`** — Opens a file in append mode, creating it if it doesn't exist. This is Rust's builder for fine-grained file open options.

**`writeln!(file, ...)`** — The `writeln!` macro writes a formatted string followed by a newline to any writer (here, a file).

---

## 13. src/recent_dirs.rs — Recent Directories

This module tracks which directories the user has created sessions in, so the new-task form can offer them as suggestions.

### 13.1 Data Structures

```rust
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
struct DirEntry {
    path: String,
    last_used: u64,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct RecentDirsDb {
    directories: Vec<DirEntry>,
}
```

Each directory entry stores the path and the Unix timestamp of when it was last used. These structs are private — the rest of the app interacts through the public API functions.

### 13.2 Recording a Directory

```rust
fn record_directory_to(path: &Path, directory: &str) -> Result<()> {
    let mut db = load_db_from(path)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    if let Some(entry) = db.directories.iter_mut().find(|e| e.path == directory) {
        entry.last_used = now;
    } else {
        db.directories.push(DirEntry {
            path: directory.to_string(),
            last_used: now,
        });
    }

    save_db_to(path, &db)?;
    Ok(())
}
```

If the directory already exists in the database, its timestamp is updated. Otherwise, a new entry is added. This is an **upsert** pattern.

**`.iter_mut().find(|e| e.path == directory)`** — Searches for a mutable reference to an existing entry. If found, its `last_used` is updated in place.

### 13.3 Retrieving Recent Directories

```rust
fn recent_directories_from(path: &Path, limit: usize) -> Result<Vec<String>> {
    let db = load_db_from(path)?;
    let mut entries = db.directories;
    entries.sort_by(|a, b| b.last_used.cmp(&a.last_used));

    let mut seen = HashSet::new();
    let mut dirs = Vec::new();
    for e in entries {
        if seen.insert(e.path.clone()) {
            dirs.push(e.path);
            if dirs.len() >= limit {
                break;
            }
        }
    }
    Ok(dirs)
}
```

Returns directories sorted by most recent first, deduplicated, limited to `limit` entries. The `HashSet` ensures each directory appears only once even if the database somehow has duplicates.

**`entries.sort_by(|a, b| b.last_used.cmp(&a.last_used))`** — Sorts in descending order by timestamp. Note `b.cmp(&a)` (not `a.cmp(&b)`) — this reverses the sort order.

**`seen.insert(e.path.clone())`** — `HashSet::insert` returns `true` if the value was newly inserted, `false` if it was already present. This combines deduplication and insertion in one call.

The data is persisted to `~/.van-damme/recent_dirs.json`, following the same load/save pattern as the session database.

---

## 14. How It All Fits Together

Here's the complete lifecycle of creating a new session:

```
┌─────────────────────────────────────────────────────────┐
│  main.rs: Program starts                                │
│    Check for --version or process-hook subcommand       │
│    tui::init() → enters raw mode, alternate screen      │
│    EventHandler::new(250) → 250ms tick rate              │
│    session::list_sessions() → loads ~/.van-damme/...     │
│    Filter to alive sessions via tmux::session_exists()   │
│    recent_dirs::recent_directories(5) → load recent dirs │
│    screen = Screen::SessionList                          │
└─────────────────┬───────────────────────────────────────┘
                  │
                  ▼
┌─────────────────────────────────────────────────────────┐
│  Main Loop                                              │
│    terminal.draw() → session_list.draw() or app.draw()  │
│    events.next() → wait for keypress or tick             │
│    Key → dispatch to current screen's handle_key()       │
│    Tick → refresh session states from DB (lightweight)   │
└─────────────────┬───────────────────────────────────────┘
                  │
                  ▼ (user presses 'n')
┌─────────────────────────────────────────────────────────┐
│  SessionListAction::NewTask                             │
│    Reload recent_dirs (may have changed since startup)  │
│    app = App::with_recent_dirs(recent) → fresh form     │
│    screen = Screen::NewTask                              │
└─────────────────┬───────────────────────────────────────┘
                  │
                  ▼ (user fills form, presses Enter)
┌─────────────────────────────────────────────────────────┐
│  Action::Submit { title, directory, prompt, claude_args }│
│    launch_session() →                                    │
│      tmux::sanitize_session_name("My Task") → "my-task" │
│      Validate: tmux installed, session name not empty,   │
│               session doesn't already exist              │
│      uuid::Uuid::new_v4() → generate claude session ID   │
│      session::add_session() → persist record FIRST       │
│        (with empty tmux_session_id placeholder)          │
│      tmux::create_session() → tmux new-session -d ...    │
│        (claude window only — no editor yet)              │
│      session::update_tmux_session_id() → fill in real ID │
│      recent_dirs::record_directory() → track directory   │
│    session_list.refresh() → reloads list                 │
│    session_list.select_by_name("my-task")               │
│    screen = Screen::SessionList                          │
└─────────────────┬───────────────────────────────────────┘
                  │
                  ▼ (Claude starts and fires SessionStart hook)
┌─────────────────────────────────────────────────────────┐
│  Claude Code calls: van-damme process-hook              │
│    process_hook::run() →                                │
│      Read JSON from stdin: {session_id, "SessionStart"} │
│      Log to ~/.van-damme/debug.log                      │
│      session::find_by_claude_session() → find record    │
│      tmux::setup_editor_window() →                      │
│        Check for worktree at .claude/worktrees/<name>   │
│        Split claude window (add terminal pane)          │
│        Create editor window (vim + shell)               │
│        Select claude window as default                  │
└─────────────────────────────────────────────────────────┘
```

The resulting tmux session has two windows:
- **claude** — Running `claude --worktree my-task --session-id <uuid>` on the left, a terminal on the right, both in the worktree directory
- **editor** — vim on the left, a shell on the right, both in the worktree directory

### Hook-Driven State Updates

While a session is active, Claude Code fires hooks that keep the session state current:

```
┌─────────────────────────────────────────────────────────┐
│  User submits a prompt in Claude                         │
│    Hook: {session_id, "UserPromptSubmit"}                │
│    → process_hook updates state to Working (⚙)           │
│    → TUI tick reads new state, shows ⚙ in session list  │
└─────────────────────────────────────────────────────────┘
                  │
                  ▼
┌─────────────────────────────────────────────────────────┐
│  Claude needs permission (file write, shell command)     │
│    Hook: {session_id, "PermissionRequest"}               │
│    → process_hook updates state to WaitingUser (⏳)      │
│    → TUI tick reads new state, shows ⏳ in session list  │
└─────────────────────────────────────────────────────────┘
                  │
                  ▼
┌─────────────────────────────────────────────────────────┐
│  Claude finishes working                                │
│    Hook: {session_id, "Stop"}                            │
│    → process_hook updates state to Idle (●)              │
│    → TUI tick reads new state, shows ● in session list  │
└─────────────────────────────────────────────────────────┘
```

---

## 15. Testing

Every module has a `#[cfg(test)] mod tests { ... }` block at the bottom.

### Test Structure

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_basic() {
        assert_eq!(sanitize_session_name("My Task"), "my-task");
    }
}
```

> **Rust concept: `#[cfg(test)]`**
> This attribute means "only compile this module when running tests." The code inside doesn't exist in the release binary — it's stripped out. `use super::*` imports everything from the parent module, including private functions. This means Rust tests can test private implementation details, unlike languages where test files are separate and can only access public APIs.
> [Read more: Testing](https://doc.rust-lang.org/book/ch11-01-writing-tests.html)

### Test Helpers

Several modules define a helper to create key events:

```rust
fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}
```

This simulates a keypress. Tests then use it to drive the application:

```rust
#[test]
fn test_tab_cycles_all_fields() {
    let mut app = App::new();
    assert_eq!(app.focused_field, InputField::Title);

    app.handle_key(key(KeyCode::Tab));
    assert_eq!(app.focused_field, InputField::Directory);

    app.handle_key(key(KeyCode::Tab));
    assert_eq!(app.focused_field, InputField::Prompt);

    app.handle_key(key(KeyCode::Tab));
    assert_eq!(app.focused_field, InputField::ClaudeArgs);

    app.handle_key(key(KeyCode::Tab));
    assert_eq!(app.focused_field, InputField::Title);
}
```

### Testing with Temporary Files

The session and recent_dirs modules use `tempfile` to avoid touching the real database during tests:

```rust
fn temp_db_path() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("sessions.json");
    (tmp, path)
}
```

**Why return `TempDir`?** — The temporary directory is automatically deleted when the `TempDir` value is dropped (goes out of scope). By returning it alongside the path, the directory lives as long as the test needs it. If we only returned the `PathBuf`, the `TempDir` would be dropped immediately and the directory would vanish.

> **Rust concept: Drop and RAII**
> When a value goes out of scope, Rust automatically calls its `Drop` implementation (if any). For `TempDir`, this means deleting the temporary directory. This pattern — tying resource cleanup to scope — is called RAII (Resource Acquisition Is Initialization). It's the same principle behind automatic file closing, mutex unlocking, etc.
> [Read more: Drop trait](https://doc.rust-lang.org/book/ch15-03-drop.html)

### Testing State Updates

The session module tests the full state lifecycle:

```rust
#[test]
fn test_update_session_state() {
    let (_tmp, path) = temp_db_path();
    add_session_to(&path, "$1".to_string(), "my-session".to_string(),
                   "uuid-1".to_string(), "/tmp".to_string()).unwrap();

    let sessions = list_sessions_from(&path).unwrap();
    assert_eq!(sessions[0].state, SessionState::Idle);

    update_state_by_claude_session_at(&path, "uuid-1", SessionState::Working).unwrap();
    let sessions = list_sessions_from(&path).unwrap();
    assert_eq!(sessions[0].state, SessionState::Working);

    update_state_by_claude_session_at(&path, "uuid-1", SessionState::WaitingUser).unwrap();
    let sessions = list_sessions_from(&path).unwrap();
    assert_eq!(sessions[0].state, SessionState::WaitingUser);
}
```

### Testing Backward Compatibility

```rust
#[test]
fn test_state_defaults_to_idle_on_deserialize() {
    let json = r#"{
        "tmux_session_id": "$1",
        "tmux_session_name": "legacy",
        "claude_session_id": null,
        "directory": "/tmp",
        "created_at": 100
    }"#;
    let record: SessionRecord = serde_json::from_str(json).unwrap();
    assert_eq!(record.state, SessionState::Idle);
}
```

This test verifies that old session records (without a `state` field) deserialize correctly thanks to the `#[serde(default = "default_state")]` attribute.

### Ignored Tests

```rust
#[test]
#[ignore]
fn test_session_exists_nonexistent() { ... }
```

Tests marked `#[ignore]` are skipped by default (run with `cargo test -- --ignored`). These are integration tests that need a running tmux server.

### Running Tests

```bash
cargo test              # Run all non-ignored tests
cargo test test_name    # Run a specific test
cargo test --lib app    # Run tests in the app module
cargo test -- --ignored # Run ignored tests too
```

---

## 16. Glossary of Rust Concepts

Quick reference for every Rust concept used in this codebase, in order of importance:

| Concept | One-liner | Book link |
|---|---|---|
| **Ownership** | Every value has one owner; when the owner goes out of scope, the value is freed | [Ch 4.1](https://doc.rust-lang.org/book/ch04-01-what-is-ownership.html) |
| **Borrowing** | `&T` reads, `&mut T` writes — compiler enforces no simultaneous mutable + immutable borrows | [Ch 4.2](https://doc.rust-lang.org/book/ch04-02-references-and-borrowing.html) |
| **Enums** | Tagged unions that can carry data in each variant | [Ch 6.1](https://doc.rust-lang.org/book/ch06-01-defining-an-enum.html) |
| **Pattern matching** | `match` exhaustively handles all enum variants | [Ch 6.2](https://doc.rust-lang.org/book/ch06-02-match.html) |
| **Result & ?** | `Result<T, E>` for recoverable errors; `?` propagates errors | [Ch 9.2](https://doc.rust-lang.org/book/ch09-02-recoverable-errors-with-result.html) |
| **Option** | `Some(T)` or `None` — replaces null | [Ch 6.1](https://doc.rust-lang.org/book/ch06-01-defining-an-enum.html#the-option-enum-and-its-advantages-over-null-values) |
| **Structs** | Named fields grouped together (like a class without methods) | [Ch 5.1](https://doc.rust-lang.org/book/ch05-01-defining-structs.html) |
| **impl blocks** | Methods and associated functions on a struct | [Ch 5.3](https://doc.rust-lang.org/book/ch05-03-method-syntax.html) |
| **Traits** | Shared behavior (interfaces) that types can implement | [Ch 10.2](https://doc.rust-lang.org/book/ch10-02-traits.html) |
| **Generics** | Type parameters like `Vec<T>` or `Result<T, E>` | [Ch 10.1](https://doc.rust-lang.org/book/ch10-01-syntax.html) |
| **Lifetimes** | Ensure references don't outlive the data they point to; `'static` for program-lifetime data | [Ch 10.3](https://doc.rust-lang.org/book/ch10-03-lifetime-syntax.html) |
| **Modules** | Code organization using `mod`, `pub`, and `use` | [Ch 7](https://doc.rust-lang.org/book/ch07-00-managing-growing-projects-with-packages-crates-and-modules.html) |
| **Closures** | Anonymous functions: `\|x\| x + 1` | [Ch 13.1](https://doc.rust-lang.org/book/ch13-01-closures.html) |
| **Iterators** | Lazy sequences with `.map()`, `.filter()`, `.collect()` | [Ch 13.2](https://doc.rust-lang.org/book/ch13-02-iterators.html) |
| **String vs &str** | Owned heap string vs borrowed string slice | [Ch 8.2](https://doc.rust-lang.org/book/ch08-02-strings.html) |
| **Vec** | Growable array | [Ch 8.1](https://doc.rust-lang.org/book/ch08-01-vectors.html) |
| **Mutability** | Variables are immutable by default; `mut` opts in | [Ch 3.1](https://doc.rust-lang.org/book/ch03-01-variables-and-mutability.html) |
| **Constants** | Compile-time values with `const` | [Ch 3.1](https://doc.rust-lang.org/book/ch03-01-variables-and-mutability.html#constants) |
| **Type aliases** | `type Tui = Terminal<...>` for readability | [Reference](https://doc.rust-lang.org/reference/items/type-aliases.html) |
| **Drop / RAII** | Automatic cleanup when values go out of scope | [Ch 15.3](https://doc.rust-lang.org/book/ch15-03-drop.html) |
| **Let chains** | `if let Some(x) = ... && condition` — conditional pattern matching with guards (Rust 2024) | [Reference](https://doc.rust-lang.org/reference/expressions/if-expr.html#if-let-expressions) |
| **Let-else** | `let Ok(x) = ... else { return; }` — bind-or-diverge | [Reference](https://doc.rust-lang.org/reference/statements.html#let-else-statements) |
| **cfg(test)** | Conditional compilation for test-only code | [Ch 11.1](https://doc.rust-lang.org/book/ch11-01-writing-tests.html) |
| **Crates** | Packages in Rust's ecosystem, managed by Cargo | [Ch 1.3](https://doc.rust-lang.org/book/ch01-03-hello-cargo.html) |
