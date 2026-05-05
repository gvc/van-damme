# Context

## SessionDb

The session store. Owns an open file handle and an exclusive `flock` for its lifetime. Callers get it via `SessionDb::open(path)`, mutate `sessions` directly, then call `save()`. The lock releases when `SessionDb` drops.

`SessionDb` is the only entry point to the session store — there are no free functions for individual operations.

## SessionRecord

A single session entry in the store. Holds tmux identity (`tmux_session_id`, `tmux_session_name`), optional Claude identity (`claude_session_id`), working `directory`, `state`, and launch metadata (`claude_command`, `model_id`).

## SessionState

Lifecycle state of a Claude session: `Working` (Claude is running), `WaitingUser` (permission request pending), `Idle` (stopped). Updated by the process hook on Claude Code hook events.

## default_db_path

Returns `~/.van-damme/sessions.json`. Public so callers pass it explicitly to `SessionDb::open`.

## DirCompleter / BranchCompleter

Two plain structs in `src/autocomplete.rs`. `DirCompleter` wraps filesystem enumeration to suggest and complete directory paths. `BranchCompleter` wraps `git::get_local_branches` to suggest and complete branch names. Both expose `complete(input) -> Option<String>` (tab press result) and `suggest(input) -> Option<String>` (ghost suffix). No shared trait — only two impls, no polymorphism needed.

## Dropdown

Reusable state for filterable, scrollable selection lists in `app.rs`. Owns `items`, `selected`, `scroll`, `visible`, `query`. Methods: `open` (populate + show), `close` (hide + reset), `filtered` (query-filtered view), `select_next`/`select_prev` (with scroll adjustment), `selected_value`, `push_query_char`/`pop_query_char`. `App` holds two: `recent_dirs_dropdown` and `branch_dropdown`.

## CommandRunner

Seam over `std::process::Command` in `tmux.rs`. Two methods: `run` (fire-and-forget) and `run_capturing` (returns stdout). `ProcessRunner` is the production impl; `FakeRunner` is the test impl. Defined inside `tmux.rs` — not shared, because only tmux needs it (git uses real temp repos in tests instead).

## SessionLauncher

Orchestrates the 4-step Claude session launch pipeline in `src/session_launcher.rs`: (1) git prep → (2) DB insert → (3) tmux create → (4) DB update with real tmux session ID. Owns rollback logic: each step failure undoes all prior steps. Holds three adapter traits (`GitAdapter`, `TmuxAdapter`, `SessionDbAdapter`) injected at construction so the pipeline is unit-testable with fakes.

## GitUndo

Return value of `git::prepare_branch` and `git::prepare_worktree`. Describes how to reverse the git state change on failure: `CheckoutBranch(original)` — just checkout back; `CheckoutAndDeleteBranch { original, created }` — checkout back and delete the newly-created branch; `Nothing` — no git changes were made. Consumed by `git::undo`.

## GitAdapter / TmuxAdapter / SessionDbAdapter

Three traits in `src/session_launcher.rs` that `SessionLauncher` depends on. Production impls: `RealGitAdapter` (delegates to `git::`), `RealTmuxAdapter` (delegates to `tmux::`), `RealSessionDb` (wraps `SessionDb` with `insert`/`remove_by_name`/`update_tmux_id` methods). Test fakes live in `session_launcher.rs` `#[cfg(test)]`.
