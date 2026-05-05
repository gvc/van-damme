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
