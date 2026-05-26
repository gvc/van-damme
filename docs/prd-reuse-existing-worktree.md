# PRD: Reuse Existing Worktree

## Problem

When a user creates a session in Worktree mode, `vd` prepares the main branch and Claude Code creates a fresh worktree. If the user later wants to return to that worktree — e.g., to continue work on the same code changes with a new Claude conversation — there is no way to do so from the TUI. The user must manually navigate to the worktree path and launch Claude themselves.

## Solution

Allow users to select an existing worktree when creating a new session. A new Claude session starts in the existing worktree directory, preserving all code changes from prior work.

## UX Flow

### Field Order Change

Current: Title → Directory → [BranchName] → Prompt → ClaudeCommand → Model

New: **Directory → Title** → [BranchName] → Prompt → ClaudeCommand → Model

Directory comes first because it determines which worktrees are available. Any change to the directory field clears the title field unconditionally.

### Selecting an Existing Worktree

1. User is on the new-task form in **Worktree** git mode.
2. User fills in the **Directory** field (repo root).
3. User moves to the **Title** field and presses **Ctrl+D**.
4. A dropdown appears listing all subdirectories found in `<directory>/.claude/worktrees/`.
5. User selects a worktree name from the list (e.g., `feat+turbo-codegen-pipeline`).
6. Title field is populated with the raw worktree directory name.
7. User fills remaining fields (prompt, model, etc.) and submits.

### Launch Behavior

On submit, the launcher detects `GitMode::ExistingWorktree` and:

1. **Skips git prep** — no stash, no checkout main, no pull.
2. **Sets working directory** to `<repo>/.claude/worktrees/<title>/`.
3. **Launches Claude without `--worktree`** — the worktree already exists, Claude runs directly in it.
4. DB insert and tmux session creation proceed as normal.

## Data Model Changes

### `GitMode` enum

Add variant:

```rust
enum GitMode {
    Worktree,          // existing — creates new worktree
    Branch,            // existing — works on branch in-place
    ExistingWorktree,  // new — reuses existing worktree directory
}
```

### Worktree Discovery

Filesystem scan of `<dir>/.claude/worktrees/`. List all subdirectories. No git health checks, no filtering by live tmux sessions.

```
fn list_worktrees(dir: &str) -> Vec<String>
```

## Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| New vs resumed Claude session | New session | Resuming conversations is fragile (expiry, stale context). Value is in the code state. User can `--continue` via CLI args field if needed. |
| Worktree discovery method | Filesystem scan | Simpler than parsing `git worktree list`. Matches what user sees on disk. No git version dependency. |
| Filter broken/prunable worktrees | No | User explicitly chose to reuse. If git state is broken, Claude handles it. |
| Filter worktrees with live tmux sessions | No | Show all. Existing tmux collision error on submit is clear enough. |
| Title clearing on directory change | Unconditional clear | No tracking flag needed. Field order (Directory → Title) makes this intuitive. |
| Tmux session name collision | Keep existing error | User likely wants to attach to existing session, not duplicate. Error message guides them. |
| Title display format | Raw worktree directory name | User recognizes it. `sanitize_session_name` converts for tmux at submit time. |

## Scope

### In Scope

- `GitMode::ExistingWorktree` variant and launcher handling
- Ctrl+D on Title field to open worktree dropdown (Worktree mode only)
- `list_worktrees()` function (filesystem scan)
- Field order swap: Directory → Title
- Unconditional title clear on directory change
- Tests for new code paths

### Out of Scope

- Resuming Claude conversations (session continuity)
- Worktree cleanup/deletion from TUI
- Worktree health indicators in dropdown

## Affected Files

| File | Change |
|------|--------|
| `src/app.rs` | `GitMode::ExistingWorktree` variant. Field order swap (Directory → Title). Ctrl+D handler on Title in Worktree mode. Title clearing on directory change. Dropdown rendering for worktree list. |
| `src/git.rs` | `list_worktrees(dir) -> Vec<String>` function. |
| `src/session_launcher.rs` | `ExistingWorktree` match arm in `launch()`: skip git prep, resolve worktree path, set `use_worktree = false`. |
| `src/main.rs` | Pass new `GitMode` variant through `spawn_launch` / `Action::Submit`. |
| `src/tmux.rs` | No changes — `use_worktree: false` already handled. |
