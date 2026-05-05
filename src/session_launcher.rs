use color_eyre::Result;

use crate::app::{GitMode, ModelSelection};
use crate::session::{SessionRecord, SessionState};
use crate::tmux::TmuxSession;

// ── Git ──────────────────────────────────────────────────────────────────────

/// Describes how to undo git state changes made during session preparation.
#[derive(Debug)]
pub enum GitUndo {
    /// Just checkout back to the original branch.
    CheckoutBranch(String),
    /// Checkout back to original branch AND delete the newly-created branch.
    CheckoutAndDeleteBranch { original: String, created: String },
    Nothing,
}

pub trait GitAdapter {
    fn prepare_worktree(&self, dir: &str, on_step: &dyn Fn(&str)) -> Result<GitUndo>;
    fn prepare_branch(&self, dir: &str, branch: &str) -> Result<GitUndo>;
    fn undo(&self, dir: &str, undo: GitUndo) -> Result<()>;
}

// ── Tmux ─────────────────────────────────────────────────────────────────────

pub trait TmuxAdapter {
    fn session_exists(&self, name: &str) -> Result<bool>;
    #[allow(clippy::too_many_arguments)]
    fn create_session(
        &self,
        name: &str,
        dir: &str,
        prompt: Option<&str>,
        claude_args: Option<&str>,
        claude_session_id: &str,
        use_worktree: bool,
        claude_command: &str,
        model_id: Option<&str>,
    ) -> Result<TmuxSession>;
    fn kill_session(&self, name: &str) -> Result<()>;
}

// ── Session DB ───────────────────────────────────────────────────────────────

pub trait SessionDbAdapter {
    fn insert(&mut self, record: SessionRecord) -> Result<()>;
    fn remove_by_name(&mut self, name: &str) -> Result<()>;
    fn update_tmux_id(&mut self, name: &str, id: &str) -> Result<()>;
}

// ── Production impls ─────────────────────────────────────────────────────────

pub struct RealGitAdapter;

impl GitAdapter for RealGitAdapter {
    fn prepare_worktree(&self, dir: &str, on_step: &dyn Fn(&str)) -> Result<GitUndo> {
        crate::git::prepare_worktree(dir, on_step)
    }

    fn prepare_branch(&self, dir: &str, branch: &str) -> Result<GitUndo> {
        crate::git::prepare_branch(dir, branch)
    }

    fn undo(&self, dir: &str, undo: GitUndo) -> Result<()> {
        crate::git::undo(dir, undo)
    }
}

pub struct RealTmuxAdapter;

impl TmuxAdapter for RealTmuxAdapter {
    fn session_exists(&self, name: &str) -> Result<bool> {
        crate::tmux::session_exists(name)
    }

    fn create_session(
        &self,
        name: &str,
        dir: &str,
        prompt: Option<&str>,
        claude_args: Option<&str>,
        claude_session_id: &str,
        use_worktree: bool,
        claude_command: &str,
        model_id: Option<&str>,
    ) -> Result<TmuxSession> {
        crate::tmux::create_session(
            name,
            dir,
            prompt,
            claude_args,
            claude_session_id,
            use_worktree,
            claude_command,
            model_id,
        )
    }

    fn kill_session(&self, name: &str) -> Result<()> {
        crate::tmux::kill_session(name)
    }
}

pub struct RealSessionDb {
    db: crate::session::SessionDb,
}

impl RealSessionDb {
    pub fn open(path: &std::path::Path) -> Result<Self> {
        let db = crate::session::SessionDb::open(path)?;
        Ok(Self { db })
    }
}

impl SessionDbAdapter for RealSessionDb {
    fn insert(&mut self, record: SessionRecord) -> Result<()> {
        self.db.sessions.push(record);
        self.db.save()
    }

    fn remove_by_name(&mut self, name: &str) -> Result<()> {
        self.db.sessions.retain(|s| s.tmux_session_name != name);
        self.db.save()
    }

    fn update_tmux_id(&mut self, name: &str, id: &str) -> Result<()> {
        if let Some(s) = self.db.sessions.iter_mut().find(|s| s.tmux_session_name == name) {
            s.tmux_session_id = id.to_string();
        }
        self.db.save()
    }
}

// ── Launcher ─────────────────────────────────────────────────────────────────

pub struct SessionLauncher<'a> {
    git: &'a dyn GitAdapter,
    tmux: &'a dyn TmuxAdapter,
    db: &'a mut dyn SessionDbAdapter,
}

impl<'a> SessionLauncher<'a> {
    pub fn new(
        git: &'a dyn GitAdapter,
        tmux: &'a dyn TmuxAdapter,
        db: &'a mut dyn SessionDbAdapter,
    ) -> Self {
        Self { git, tmux, db }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn launch(
        &mut self,
        title: &str,
        directory: &str,
        git_mode: GitMode,
        branch_name: Option<&str>,
        prompt: Option<&str>,
        claude_command: &str,
        model_selection: ModelSelection,
        claude_session_id: &str,
        on_step: &dyn Fn(&str),
    ) -> Result<()> {
        let session_name = crate::tmux::sanitize_session_name(title);

        if session_name.is_empty() {
            return Err(color_eyre::eyre::eyre!(
                "Title '{title}' produces an empty session name"
            ));
        }

        if self.tmux.session_exists(&session_name)? {
            return Err(color_eyre::eyre::eyre!(
                "tmux session '{session_name}' already exists"
            ));
        }

        // Step 1: git prep
        let use_worktree = git_mode == GitMode::Worktree;
        let git_undo = match git_mode {
            GitMode::Worktree => self.git.prepare_worktree(directory, on_step)?,
            GitMode::Branch => {
                if let Some(branch) = branch_name {
                    on_step(&format!("Preparing branch '{branch}'..."));
                    let undo = self.git.prepare_branch(directory, branch)?;
                    on_step(&format!("Branch '{branch}' ready"));
                    undo
                } else {
                    GitUndo::Nothing
                }
            }
        };

        let dir = {
            let t = directory.trim_end_matches('/');
            if t.is_empty() { "/".to_string() } else { t.to_string() }
        };

        let created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Step 2: DB insert (placeholder tmux_session_id — updated after tmux creates session)
        let record = SessionRecord {
            tmux_session_id: String::new(),
            tmux_session_name: session_name.clone(),
            claude_session_id: Some(claude_session_id.to_string()),
            directory: dir,
            created_at,
            state: SessionState::Idle,
            claude_command: claude_command.to_string(),
            model_id: model_selection.model_id().map(str::to_string),
        };

        if let Err(e) = self.db.insert(record) {
            let _ = self.git.undo(directory, git_undo);
            return Err(e);
        }

        on_step("Creating tmux session...");

        // Step 3: tmux create
        let tmux_session = match self.tmux.create_session(
            &session_name,
            directory,
            prompt,
            None,
            claude_session_id,
            use_worktree,
            claude_command,
            model_selection.model_id(),
        ) {
            Ok(s) => s,
            Err(e) => {
                let _ = self.db.remove_by_name(&session_name);
                let _ = self.git.undo(directory, git_undo);
                return Err(e);
            }
        };

        // Step 4: DB update with real tmux session ID
        if let Err(e) = self.db.update_tmux_id(&session_name, &tmux_session.session_id) {
            let _ = self.tmux.kill_session(&session_name);
            let _ = self.db.remove_by_name(&session_name);
            let _ = self.git.undo(directory, git_undo);
            return Err(e);
        }

        on_step("Session launched");
        Ok(())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    // ── Fakes ────────────────────────────────────────────────────────────────

    struct FakeGit {
        prepare_result: Result<GitUndo>,
        undo_calls: RefCell<Vec<String>>,
    }

    impl FakeGit {
        fn ok() -> Self {
            Self {
                prepare_result: Ok(GitUndo::Nothing),
                undo_calls: RefCell::new(vec![]),
            }
        }
        fn failing() -> Self {
            Self {
                prepare_result: Err(color_eyre::eyre::eyre!("git failed")),
                undo_calls: RefCell::new(vec![]),
            }
        }
        fn with_undo(original: &str) -> Self {
            Self {
                prepare_result: Ok(GitUndo::CheckoutBranch(original.to_string())),
                undo_calls: RefCell::new(vec![]),
            }
        }
        fn undo_called(&self) -> bool {
            !self.undo_calls.borrow().is_empty()
        }
    }

    impl GitAdapter for FakeGit {
        fn prepare_worktree(&self, _dir: &str, _on_step: &dyn Fn(&str)) -> Result<GitUndo> {
            match &self.prepare_result {
                Ok(_) => Ok(GitUndo::Nothing),
                Err(_) => Err(color_eyre::eyre::eyre!("git failed")),
            }
        }
        fn prepare_branch(&self, _dir: &str, _branch: &str) -> Result<GitUndo> {
            match &self.prepare_result {
                Ok(_) => Ok(GitUndo::CheckoutBranch("main".to_string())),
                Err(_) => Err(color_eyre::eyre::eyre!("git failed")),
            }
        }
        fn undo(&self, _dir: &str, undo: GitUndo) -> Result<()> {
            self.undo_calls.borrow_mut().push(format!("{undo:?}"));
            Ok(())
        }
    }

    struct FakeTmux {
        exists: bool,
        create_result: Result<TmuxSession>,
        kill_calls: RefCell<Vec<String>>,
    }

    impl FakeTmux {
        fn ok() -> Self {
            Self {
                exists: false,
                create_result: Ok(TmuxSession { session_id: "$1".to_string() }),
                kill_calls: RefCell::new(vec![]),
            }
        }
        fn failing() -> Self {
            Self {
                exists: false,
                create_result: Err(color_eyre::eyre::eyre!("tmux failed")),
                kill_calls: RefCell::new(vec![]),
            }
        }
        fn kill_called(&self) -> bool {
            !self.kill_calls.borrow().is_empty()
        }
    }

    impl TmuxAdapter for FakeTmux {
        fn session_exists(&self, _name: &str) -> Result<bool> {
            Ok(self.exists)
        }
        fn create_session(
            &self, _name: &str, _dir: &str, _prompt: Option<&str>,
            _claude_args: Option<&str>, _claude_session_id: &str, _use_worktree: bool,
            _claude_command: &str, _model_id: Option<&str>,
        ) -> Result<TmuxSession> {
            match &self.create_result {
                Ok(s) => Ok(TmuxSession { session_id: s.session_id.clone() }),
                Err(_) => Err(color_eyre::eyre::eyre!("tmux failed")),
            }
        }
        fn kill_session(&self, name: &str) -> Result<()> {
            self.kill_calls.borrow_mut().push(name.to_string());
            Ok(())
        }
    }

    #[derive(Default)]
    struct FakeDb {
        sessions: Vec<SessionRecord>,
        fail_update: bool,
    }

    impl FakeDb {
        fn ok() -> Self { Self::default() }
        fn failing_update() -> Self { Self { fail_update: true, ..Default::default() } }
    }

    impl SessionDbAdapter for FakeDb {
        fn insert(&mut self, record: SessionRecord) -> Result<()> {
            self.sessions.push(record);
            Ok(())
        }
        fn remove_by_name(&mut self, name: &str) -> Result<()> {
            self.sessions.retain(|s| s.tmux_session_name != name);
            Ok(())
        }
        fn update_tmux_id(&mut self, name: &str, id: &str) -> Result<()> {
            if self.fail_update {
                return Err(color_eyre::eyre::eyre!("db update failed"));
            }
            if let Some(s) = self.sessions.iter_mut().find(|s| s.tmux_session_name == name) {
                s.tmux_session_id = id.to_string();
            }
            Ok(())
        }
    }

    fn launch(
        git: &dyn GitAdapter,
        tmux: &dyn TmuxAdapter,
        db: &mut dyn SessionDbAdapter,
    ) -> Result<()> {
        SessionLauncher::new(git, tmux, db).launch(
            "test",
            "/tmp",
            GitMode::Worktree,
            None,
            None,
            "claude",
            ModelSelection::Default,
            "uuid-1",
            &|_| {},
        )
    }

    // ── Scenario tests ────────────────────────────────────────────────────────

    #[test]
    fn happy_path_inserts_and_updates_db() {
        let git = FakeGit::ok();
        let tmux = FakeTmux::ok();
        let mut db = FakeDb::ok();

        launch(&git, &tmux, &mut db).unwrap();

        assert_eq!(db.sessions.len(), 1);
        assert_eq!(db.sessions[0].tmux_session_id, "$1");
        assert_eq!(db.sessions[0].tmux_session_name, "test");
    }

    #[test]
    fn git_failure_does_not_insert_db() {
        let git = FakeGit::failing();
        let tmux = FakeTmux::ok();
        let mut db = FakeDb::ok();

        assert!(launch(&git, &tmux, &mut db).is_err());
        assert!(db.sessions.is_empty());
    }

    #[test]
    fn tmux_failure_rolls_back_db_and_git() {
        let git = FakeGit::with_undo("main");
        let tmux = FakeTmux::failing();
        let mut db = FakeDb::ok();

        assert!(launch(&git, &tmux, &mut db).is_err());
        assert!(db.sessions.is_empty(), "DB record not removed");
        assert!(git.undo_called(), "git undo not called");
    }

    #[test]
    fn db_update_failure_kills_tmux_and_rolls_back() {
        let git = FakeGit::with_undo("main");
        let tmux = FakeTmux::ok();
        let mut db = FakeDb::failing_update();

        assert!(launch(&git, &tmux, &mut db).is_err());
        assert!(tmux.kill_called(), "tmux session not killed");
        assert!(git.undo_called(), "git undo not called");
    }

    #[test]
    fn existing_session_returns_error_without_any_side_effects() {
        let git = FakeGit::ok();
        let mut tmux = FakeTmux::ok();
        tmux.exists = true;
        let mut db = FakeDb::ok();

        let err = launch(&git, &tmux, &mut db).unwrap_err();
        assert!(err.to_string().contains("already exists"));
        assert!(db.sessions.is_empty());
    }
}
