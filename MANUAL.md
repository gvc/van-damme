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
12. [How It All Fits Together](#12-how-it-all-fits-together)
13. [Testing](#13-testing)
14. [Glossary of Rust Concepts](#14-glossary-of-rust-concepts)

---

## 1. What the Application Does

Van Damme is a session manager for Claude Code in tmux. It presents a full-screen terminal interface where you can:

- **Browse** your active coding sessions
- **Create** a new session (which spawns a tmux window running Claude and another running your editor)
- **Attach** to an existing session
- **Kill** sessions you no longer need

All session data is persisted to `~/.van-damme/sessions.json`.

---

## 2. Project Structure

```
van-damme/
├── Cargo.toml            # Project manifest and dependencies
└── src/
    ├── main.rs           # Entry point, main loop, screen routing
    ├── tui.rs            # Terminal initialization and cleanup
    ├── event.rs          # Keyboard event polling
    ├── theme.rs          # Color constants
    ├── app.rs            # "New Task" form (input, validation, rendering)
    ├── session_list.rs   # Session list screen (navigation, rendering)
    ├── tmux.rs           # Shell commands to create/kill tmux sessions
    └── session.rs        # JSON file read/write for session records
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
version = "0.1.0"
edition = "2024"

[dependencies]
ratatui = "0.29"
crossterm = "0.28"
color-eyre = "0.6"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
dirs = "6"
tui-input = "0.11"

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
| `tempfile` (dev only) | Creates temporary files and directories for tests |

> **Rust concept: Crates**
> A "crate" is Rust's term for a package or library. `[dependencies]` lists crates your code uses at runtime. `[dev-dependencies]` lists crates only needed for testing.
> [Read more: Cargo and Crates](https://doc.rust-lang.org/book/ch01-03-hello-cargo.html)

The `edition = "2024"` line sets which version of the Rust language to use. Editions introduce new syntax and features without breaking old code.

The `features = ["derive"]` on serde enables a macro that auto-generates serialization code for your structs (more on this in the session module).

---

## 4. src/main.rs — The Entry Point

This is where the program starts. Let's walk through it section by section.

### 4.1 Module Declarations

```rust
mod app;
mod event;
mod session;
mod session_list;
pub mod theme;
mod tmux;
mod tui;
```

Each `mod` line tells the compiler: "there's a module with this name — go find the `.rs` file and include it." The `pub` on `theme` makes it accessible from outside this crate (although for a binary crate like this one, it mainly means other modules can use `crate::theme` paths — all `mod` declarations in `main.rs` are accessible to sibling modules regardless).

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
    color_eyre::install()?;

    let mut terminal = tui::init()?;
    let events = EventHandler::new(250);
    // ...
```

**`fn main() -> Result<()>`** — The entry point returns a `Result`. In Rust, functions that can fail return `Result<T, E>`, where `T` is the success type and `E` is the error type. Here, `Result<()>` means "either succeed with nothing (`()`, pronounced 'unit', Rust's void) or fail with an error."

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
let alive: Vec<_> = sessions
    .into_iter()
    .filter(|s| tmux::session_exists(&s.tmux_session_name).unwrap_or(false))
    .collect();
```

This loads saved sessions from disk, then filters them down to only those that are actually still running in tmux.

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
                            SessionListAction::NewTask => { /* ... */ }
                            SessionListAction::Attach { session_name } => { /* ... */ }
                            SessionListAction::None => {}
                        }
                    }
                    Screen::NewTask => {
                        let action = app.handle_key(key);
                        match action {
                            Action::Submit { title, directory, prompt } => { /* ... */ }
                            Action::Quit => { /* ... */ }
                            Action::None => {}
                        }
                    }
                }
            }
        }
        Event::Tick => {}
    }
}
```

This is the heartbeat of the application. Every TUI follows the same pattern: **draw, wait for input, update state, repeat**.

1. **Draw** — `terminal.draw(|frame| { ... })` gives you a `frame` to paint widgets onto. The closure fills the whole screen with the background color, then delegates to whichever screen is active.

2. **Wait for input** — `events.next()?` blocks until a key is pressed or the tick timer fires.

3. **Update state** — The key event is dispatched to the current screen's `handle_key` method, which returns an `Action` describing what should happen.

> **Rust concept: `match`**
> `match` is Rust's pattern matching — like a `switch` statement on steroids. Unlike `switch`, `match` is **exhaustive**: the compiler forces you to handle every possible variant of an enum. If you add a new variant to `Action` but forget to handle it in a `match`, your code won't compile. This eliminates entire categories of bugs.
> [Read more: match](https://doc.rust-lang.org/book/ch06-02-match.html)

Notice the pattern `Action::Submit { title, directory, prompt }` — this **destructures** the enum variant, pulling out the `title`, `directory`, and `prompt` fields into local variables. This is how Rust's enums carry data: the variant holds fields, and `match` extracts them.

### 4.7 Launching and Attaching Sessions

```rust
fn launch_session(title: &str, directory: &str, prompt: Option<&str>) -> Result<()> {
    let session_name = tmux::sanitize_session_name(title);

    if session_name.is_empty() {
        return Err(color_eyre::eyre::eyre!(
            "Title '{title}' produces an empty session name"
        ));
    }
    // ...
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

---

## 7. src/theme.rs — The Color Palette

```rust
use ratatui::style::Color;

pub const BG: Color = Color::Rgb(53, 56, 63);
pub const ORANGE: Color = Color::Rgb(200, 90, 26);
pub const ORANGE_BRIGHT: Color = Color::Rgb(220, 120, 40);
pub const BLUE: Color = Color::Rgb(74, 106, 138);
pub const GRAY: Color = Color::Rgb(60, 60, 80);
pub const GRAY_DIM: Color = Color::Rgb(65, 137, 181);
pub const TEXT: Color = Color::Rgb(180, 180, 195);
pub const ERROR: Color = Color::Rgb(200, 60, 60);
pub const SESSION_NAME: Color = Color::Rgb(249, 217, 67);
```

> **Rust concept: Constants**
> `const` defines compile-time constants. They must have an explicit type and their value must be computable at compile time. Unlike `let` bindings, constants can be used at the module level (outside functions) and are inlined wherever they're used.
> [Read more: Constants](https://doc.rust-lang.org/book/ch03-01-variables-and-mutability.html#constants)

`Color::Rgb(r, g, b)` is an enum variant that holds three `u8` values (0-255 each). All colors in this file use 24-bit RGB, meaning the terminal must support true color (most modern terminals do).

These constants are used throughout the rendering code. By centralizing them, changing the look of the entire application means editing only this file.

---

## 8. src/app.rs — The "New Task" Form

This is the largest module. It handles a three-field form (title, directory, prompt), keyboard navigation, path autocompletion, validation, and rendering.

### 8.1 Imports

```rust
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Position},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use std::path::Path;
use tui_input::Input;
use tui_input::backend::crossterm::EventHandler;

use crate::theme;
```

`crate::theme` means "the `theme` module from the root of this crate." Since `theme` is declared in `main.rs` (the crate root), all other modules access it through `crate::`.

### 8.2 Directory Tab-Completion

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

### 8.3 Longest Common Prefix

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

### 8.4 The Data Types

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputField {
    Title,
    Directory,
    Prompt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    None,
    Quit,
    Submit {
        title: String,
        directory: String,
        prompt: Option<String>,
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

### 8.5 The App Struct

```rust
#[derive(Debug)]
pub struct App {
    pub running: bool,
    pub focused_field: InputField,
    pub title_input: Input,
    pub dir_input: Input,
    pub prompt_input: Input,
    pub dir_suggestion: Option<String>,
    pub error_message: Option<String>,
}
```

This struct holds all state for the "New Task" form. `Input` is from the `tui-input` crate — a stateful text input widget.

### 8.6 The Constructor

```rust
impl App {
    pub fn new() -> Self {
        let cwd = std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        Self {
            running: true,
            focused_field: InputField::Title,
            title_input: Input::default(),
            dir_input: Input::new(cwd),
            prompt_input: Input::default(),
            dir_suggestion: None,
            error_message: None,
        }
    }
```

**`.map(|p| ...)`** — `map` transforms the value inside a `Result` (or `Option`) if it's successful, leaving errors untouched. Here, if `current_dir()` succeeds, the path is converted to a `String`.

**`Input::default()`** — The `Default` trait provides a "zero value" constructor. For `Input`, this means an empty text field.

### 8.7 Keyboard Handling

```rust
pub fn handle_key(&mut self, key: KeyEvent) -> Action {
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

The `_ =>` arm is the catch-all: any key that doesn't match the specific cases above is forwarded to the currently focused input widget.

### 8.8 Form Validation

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
    let prompt = if prompt_raw.is_empty() {
        None
    } else {
        Some(prompt_raw)
    };

    Action::Submit {
        title,
        directory,
        prompt,
    }
}
```

**`.trim().to_string()`** — `.trim()` returns a `&str` (a slice with leading/trailing whitespace removed). `.to_string()` creates an owned `String` from it. This is needed because the `Action::Submit` variant owns its `String` fields.

> **Rust concept: String vs &str**
> Rust has two main string types:
> - `String` — An owned, heap-allocated, growable string. You can modify it, store it in structs, return it from functions.
> - `&str` — A borrowed reference to string data. Lightweight, but you can't store it without a lifetime annotation.
>
> Most functions take `&str` as input (flexible — accepts both types) and return `String` when they need to give ownership to the caller.
> [Read more: Strings](https://doc.rust-lang.org/book/ch08-02-strings.html)

**`format!("Directory does not exist: {directory}")`** — The `format!` macro creates a `String` with interpolated values. The `{directory}` syntax embeds the variable directly (Rust 2021+ feature, similar to JavaScript template literals).

### 8.9 Drawing the Form

```rust
pub fn draw(&self, frame: &mut Frame) {
    let area = frame.area();

    let form_width = 60u16.min(area.width.saturating_sub(2));
    let form_height = 16u16.min(area.height.saturating_sub(2));

    let vertical = Layout::vertical([Constraint::Length(form_height)])
        .flex(Flex::Center)
        .split(area);
    let horizontal = Layout::horizontal([Constraint::Length(form_width)])
        .flex(Flex::Center)
        .split(vertical[0]);
    let form_area = horizontal[0];
```

**`60u16`** — The `u16` suffix specifies the type of the literal. `u16` is an unsigned 16-bit integer. ratatui uses `u16` for screen coordinates because terminal dimensions never exceed 65,535.

**`.saturating_sub(2)`** — Subtraction that stops at zero instead of overflowing. Since `u16` is unsigned, `0 - 2` would panic or wrap around. Saturating subtraction prevents this: `1.saturating_sub(2)` gives `0`.

**Layout and Constraint** — ratatui's layout system works by splitting a rectangular area into chunks:
- `Layout::vertical([...])` splits vertically
- `Layout::horizontal([...])` splits horizontally
- `Constraint::Length(n)` requests exactly `n` cells
- `Flex::Center` centers the content within the available space

The form inner layout follows the same pattern:

```rust
let chunks = Layout::vertical([
    Constraint::Length(1), // Title label
    Constraint::Length(3), // Title input
    Constraint::Length(1), // Directory label
    Constraint::Length(3), // Directory input
    Constraint::Length(1), // Prompt label
    Constraint::Length(3), // Prompt input
    Constraint::Min(1),    // Hints + error
])
.split(inner);
```

`Constraint::Min(1)` means "at least 1 row, but take up any remaining space."

The rendering then places widgets into these chunks:

```rust
let title_border_color = if self.focused_field == InputField::Title {
    theme::ORANGE_BRIGHT
} else {
    theme::GRAY
};
let title_block = Block::default()
    .borders(Borders::ALL)
    .border_style(Style::default().fg(title_border_color))
    .style(Style::default().bg(theme::BG));
```

Each input field gets a `Block` (a bordered rectangle) whose border color changes based on whether it's focused. This is the ratatui builder pattern: methods chain to configure a widget, then `frame.render_widget()` draws it.

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
            if self.sessions.is_empty() {
                self.list_state.select(None);
            } else if let Some(i) = self.list_state.selected()
                && i >= self.sessions.len()
            {
                self.list_state.select(Some(self.sessions.len() - 1));
            }
        }
        Err(e) => {
            self.status_message = Some(format!("Error loading sessions: {e}"));
        }
    }
}
```

The `if let Some(i) = ... && i >= ...` syntax combines an `if let` with a boolean condition. This is a **let chain** (stabilized in Rust 2024 edition). It reads: "if there's a selected index `i`, AND that index is past the end of the list, then clamp it to the last valid index."

### 9.3 Wrapping Navigation

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

### 9.4 Killing a Session

```rust
fn kill_selected(&mut self) {
    if let Some(i) = self.list_state.selected() {
        let session = &self.sessions[i];
        let name = session.tmux_session_name.clone();
        match tmux::kill_session(&name) {
            Ok(()) => {
                let _ = crate::session::remove_session(&name);
                self.status_message = Some(format!("Killed session: {name}"));
                self.refresh();
            }
            Err(e) => {
                self.status_message = Some(format!("Failed to kill '{name}': {e}"));
            }
        }
    }
}
```

**`.clone()`** — Creates a deep copy of the `String`. This is needed because `session` is borrowed from `self.sessions`, and we need the name to outlive that borrow (since `self.refresh()` will mutate `self.sessions`).

**`let _ = crate::session::remove_session(&name);`** — The `let _ =` pattern deliberately discards the result. The underscore tells the compiler (and the reader) "I know this returns a `Result`, and I'm intentionally ignoring it." This is used here because if removing from the JSON file fails, the tmux session is already dead — there's no useful recovery.

### 9.5 Rendering the Session List

```rust
let items: Vec<ListItem> = self
    .sessions
    .iter()
    .map(|s| {
        let line = Line::from(vec![
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

**`.iter()`** — Creates an iterator that borrows each element (unlike `.into_iter()` which would consume the vector). This is important because we need `self.sessions` to survive — we're just reading from it.

**`.map(|s| { ... })`** — Transforms each `SessionRecord` into a `ListItem` widget.

**`render_stateful_widget`** — Unlike `render_widget`, this takes a mutable reference to state (`&mut self.list_state`). ratatui uses this to track scroll position and selection state across frames.

Each session displays its name in golden yellow and its directory path in dimmed blue, with `"▸ "` marking the selected item.

---

## 10. src/tmux.rs — Talking to tmux

This module shells out to the `tmux` command-line tool.

### 10.1 The Session Struct

```rust
#[derive(Debug)]
pub struct TmuxSession {
    pub session_name: String,
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
pub fn create_session(name: &str, dir: &str, prompt: Option<&str>) -> Result<TmuxSession> {
    let claude_cmd = match prompt {
        Some(p) => format!("claude --worktree {name} {}", shell_escape(p)),
        None => format!("claude --worktree {name}"),
    };

    let status = Command::new("tmux")
        .args([
            "new-session", "-d", "-s", name,
            "-n", "claude",
            "-c", dir,
            &claude_cmd,
        ])
        .status()?;
    if !status.success() {
        return Err(eyre!("Failed to create tmux session '{name}'"));
    }
    // ... creates editor window, splits, captures session ID
```

**`Command::new("tmux")`** — Rust's standard library provides `std::process::Command` for spawning child processes. It's a builder pattern:
- `.args([...])` — Adds arguments
- `.status()` — Runs the command, waits for it to finish, and returns its exit status

This creates a detached tmux session (`-d`) with name (`-s`), a window called "claude" (`-n`), working directory (`-c`), and runs the `claude` CLI as the initial command.

The function then creates a second window called "editor" that opens vim in the worktree directory, and splits it horizontally with another pane:

```rust
let worktree_dir = format!("{dir}/.claude/{name}");
std::fs::create_dir_all(&worktree_dir).ok();

run_tmux(&["new-window", "-t", name, "-n", "editor", "-c", &worktree_dir])?;
run_tmux(&["send-keys", "-t", &format!("{name}:editor"), "vim .", "Enter"])?;
run_tmux(&["split-window", "-h", "-t", &format!("{name}:editor"), "-c", &worktree_dir])?;
```

### 10.4 Shell Escaping

```rust
fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
```

Wraps a string in single quotes for safe shell embedding. The trick `'\\''` ends the current single-quoted string, adds an escaped single quote, and starts a new single-quoted string. This is the standard POSIX way to include a literal `'` inside a single-quoted string.

### 10.5 The Helper Function

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

### 11.1 Data Structures with Serde

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SessionRecord {
    pub tmux_session_id: String,
    pub tmux_session_name: String,
    pub claude_session_id: Option<String>,
    pub directory: String,
    pub created_at: u64,
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

**`Default`** on `SessionDb` — The `Default` trait provides a default value. For `SessionDb`, the default is an empty `sessions` vector.

### 11.2 Loading and Saving

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

### 11.3 Adding a Session

```rust
fn add_session_to(
    path: &Path,
    tmux_session_id: String,
    tmux_session_name: String,
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
        claude_session_id: None,
        directory,
        created_at,
    };

    db.sessions.push(record.clone());
    save_db_to(path, &db)?;
    Ok(record)
}
```

**`SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()`** — Gets the current Unix timestamp (seconds since January 1, 1970). The `.unwrap()` is safe here because the system clock can only be before UNIX_EPOCH if something is seriously wrong.

Notice the function parameters `tmux_session_id: String` take ownership (no `&`). The caller gives up these strings, and they become part of the `SessionRecord`. This is an intentional design choice: the record needs to own its data for long-term storage.

> **Rust concept: Ownership transfer in function parameters**
> When a function takes `String` (not `&String` or `&str`), the caller's value is **moved** into the function. The caller can no longer use it after the call. This is how Rust ensures there's always exactly one owner of heap data.
> [Read more: Ownership and Functions](https://doc.rust-lang.org/book/ch04-01-what-is-ownership.html#ownership-and-functions)

### 11.4 Removing a Session

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

### 11.5 The Public/Private Split

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

Each operation has a public version (uses the default path) and a private `_from` version (takes a path parameter). The public version is the API that the rest of the application uses. The `_from` version is used in tests, where a temporary file path is injected instead of the real `~/.van-damme/sessions.json`. This is **dependency injection** through function parameters.

> **Rust concept: Visibility**
> `pub` makes an item public (accessible from outside the module). Without `pub`, items are private to their module. In this module, `list_sessions` is public, but `list_sessions_from` is private — only test code within this same module can call it.
> [Read more: Visibility](https://doc.rust-lang.org/book/ch07-03-paths-for-referring-to-an-item-in-the-module-tree.html)

---

## 12. How It All Fits Together

Here's the complete lifecycle of creating a new session:

```
┌─────────────────────────────────────────────────────────┐
│  main.rs: Program starts                                │
│    tui::init() → enters raw mode, alternate screen      │
│    EventHandler::new(250) → 250ms tick rate              │
│    session::list_sessions() → loads ~/.van-damme/...     │
│    Filter to alive sessions via tmux::session_exists()   │
│    screen = Screen::SessionList                          │
└─────────────────┬───────────────────────────────────────┘
                  │
                  ▼
┌─────────────────────────────────────────────────────────┐
│  Main Loop                                              │
│    terminal.draw() → session_list.draw() or app.draw()  │
│    events.next() → wait for keypress or tick             │
│    Dispatch to current screen's handle_key()             │
└─────────────────┬───────────────────────────────────────┘
                  │
                  ▼ (user presses 'n')
┌─────────────────────────────────────────────────────────┐
│  SessionListAction::NewTask                             │
│    app = App::new() → fresh form, CWD as directory      │
│    screen = Screen::NewTask                              │
└─────────────────┬───────────────────────────────────────┘
                  │
                  ▼ (user fills form, presses Enter)
┌─────────────────────────────────────────────────────────┐
│  Action::Submit { title, directory, prompt }             │
│    launch_session() →                                    │
│      tmux::sanitize_session_name("My Task") → "my-task" │
│      tmux::create_session("my-task", "/path", prompt) →  │
│        tmux new-session -d -s my-task ...                │
│        tmux new-window (editor)                          │
│        tmux split-window                                 │
│      session::add_session() → writes to JSON             │
│    session_list.refresh() → reloads list                 │
│    screen = Screen::SessionList                          │
└─────────────────────────────────────────────────────────┘
```

The resulting tmux session has two windows:
- **claude** — Running `claude --worktree my-task` (optionally with a prompt)
- **editor** — vim on the left, a shell on the right, both in the worktree directory

---

## 13. Testing

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
    assert_eq!(app.focused_field, InputField::Title);
}
```

### Testing with Temporary Files

The session module uses `tempfile` to avoid touching the real database during tests:

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

## 14. Glossary of Rust Concepts

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
| **Modules** | Code organization using `mod`, `pub`, and `use` | [Ch 7](https://doc.rust-lang.org/book/ch07-00-managing-growing-projects-with-packages-crates-and-modules.html) |
| **Closures** | Anonymous functions: `\|x\| x + 1` | [Ch 13.1](https://doc.rust-lang.org/book/ch13-01-closures.html) |
| **Iterators** | Lazy sequences with `.map()`, `.filter()`, `.collect()` | [Ch 13.2](https://doc.rust-lang.org/book/ch13-02-iterators.html) |
| **String vs &str** | Owned heap string vs borrowed string slice | [Ch 8.2](https://doc.rust-lang.org/book/ch08-02-strings.html) |
| **Vec** | Growable array | [Ch 8.1](https://doc.rust-lang.org/book/ch08-01-vectors.html) |
| **Mutability** | Variables are immutable by default; `mut` opts in | [Ch 3.1](https://doc.rust-lang.org/book/ch03-01-variables-and-mutability.html) |
| **Constants** | Compile-time values with `const` | [Ch 3.1](https://doc.rust-lang.org/book/ch03-01-variables-and-mutability.html#constants) |
| **Type aliases** | `type Tui = Terminal<...>` for readability | [Reference](https://doc.rust-lang.org/reference/items/type-aliases.html) |
| **Drop / RAII** | Automatic cleanup when values go out of scope | [Ch 15.3](https://doc.rust-lang.org/book/ch15-03-drop.html) |
| **cfg(test)** | Conditional compilation for test-only code | [Ch 11.1](https://doc.rust-lang.org/book/ch11-01-writing-tests.html) |
| **Crates** | Packages in Rust's ecosystem, managed by Cargo | [Ch 1.3](https://doc.rust-lang.org/book/ch01-03-hello-cargo.html) |
