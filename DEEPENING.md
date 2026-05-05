# Deepening Opportunities

Architectural refactors that turn shallow modules into deep ones — more leverage at the interface, better locality, better testability.

## 1. Split `app.rs` (2890 lines) into deep modules

**Status:** Open

**Files:** `src/app.rs`

**Problem:** 7 concerns in one module — form state, keyboard dispatch, directory autocomplete, branch autocomplete, recent-dirs dropdown, model selection, rendering. Autocomplete calls `std::fs::read_dir` and `git::get_local_branches` directly — no seam, untestable without real filesystem/git. Locality is poor: understanding "what happens when user tabs in directory field" requires tracing ~5 methods spread across the file.

**Solution:** Extract autocomplete logic (directory + branch) into a module with a testable interface. Extract recent-dirs dropdown into its own widget. Keep `App` as thin orchestrator.

**Benefits:** Autocomplete becomes unit-testable (inject a path-lister instead of hitting filesystem). Each extracted module has better locality. `App` becomes a shallow coordinator rather than a deep monolith.

---

## 2. Session launch pipeline in `main.rs` needs a seam

**Status:** Open

**Files:** `src/main.rs` (lines 366–460), `src/session.rs`, `src/tmux.rs`, `src/git.rs`

**Problem:** `launch_session` orchestrates git prep → DB insert → tmux creation → DB update → record directory, with manual rollback (`let _ = session::remove_session(...)`) on failure. No seam — callers must know the exact sequence. `#[allow(clippy::too_many_arguments)]` hints at coupling. Cannot test launch flow without real tmux + git + filesystem.

**Solution:** Extract a `SessionLauncher` that owns the sequence and rollback. Give it adapters for tmux/git/session-db so the pipeline is testable with fakes.

**Benefits:** Locality — rollback logic lives next to the operations it undoes. Leverage — callers get "launch a session" without knowing the 6-step sequence. Testable without subprocesses.

---

## 3. `session.rs` — load-save-load-save on every operation

**Status:** Open

**Files:** `src/session.rs`

**Problem:** Every public function (`add_session`, `remove_session`, `update_state_by_claude_session`, `update_tmux_session_id`) independently calls `load_db_from()` then `save_db_to()`. N operations = N file reads + N file writes. No batching, no caching. Concurrent writes from hook process and main process can silently overwrite each other.

**Solution:** Deepen the interface: callers get `SessionDb::open()` / `.save()` with file locking (`flock`). Multiple mutations happen on an in-memory `SessionDb` with a single save.

**Benefits:** Leverage — callers stop worrying about I/O per operation. Concurrent access becomes safe. Locality — locking/caching logic in one place.

---

## 4. `tmux.rs` — no seam over subprocess calls

**Status:** Done

**Files:** `src/tmux.rs`

**Problem:** `tmux.rs` called `std::process::Command` directly in every function. No way to test `session_exists`, `session_summary`, `create_session`, `capture_pane` without a running tmux daemon. Two integration tests were `#[ignore]`d for this reason.

`git.rs` was excluded — its tests use real temp git repos via `init_test_repo()`, which gives real coverage without mock/prod divergence risk.

**Solution:** `CommandRunner` trait with two methods — `run` (fire-and-forget) and `run_capturing` (returns stdout). Defined inside `tmux.rs`. `ProcessRunner` is the production impl; `FakeRunner` is the test impl with canned responses. `shell_escape` stays private in `tmux.rs` (single caller).

**Benefits:** Leverage — `session_exists`, `session_summary`, `capture_pane`, and `create_session` are now unit-testable without a tmux daemon. `#[ignore]` integration tests kept as a safety net for real tmux API changes.

---

## 5. `session_list.rs` — display-row index mapping

**Status:** Done

**Files:** `src/session_list.rs`, `src/grouped_list.rs`

Extracted `GroupedList<T>` — owns grouping, selection, navigation, collapse. SessionList is now a thin wrapper. All index translation eliminated.
