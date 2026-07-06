//! Log-tape and ↑/↓ input-history persistence (mirrors the Swift `LogStore`):
//! the per-user data dir, and the load/save of the running log + recall tape.

use super::*;

impl Session {
    // MARK: Log + input-history persistence (mirrors the Swift LogStore)

    /// The newest entries kept on disk — matches Swift's `LogStore.limit`.
    const LOG_LIMIT: usize = 500;

    /// The per-user data directory (`…/Application Support/Soroban` on macOS,
    /// `%APPDATA%\Soroban` on Windows, `~/.local/share/soroban` on Linux),
    /// created on demand. `None` if the platform has no data dir. The
    /// `SOROBAN_DATA_DIR` env var overrides it (an escape hatch, and the seam
    /// the persistence round-trip test points at a temp dir).
    fn data_dir() -> Option<PathBuf> {
        let dir = match std::env::var_os("SOROBAN_DATA_DIR") {
            Some(custom) => PathBuf::from(custom),
            None => dirs::data_dir()?.join("Soroban"),
        };
        std::fs::create_dir_all(&dir).ok()?;
        Some(dir)
    }

    fn log_path() -> Option<PathBuf> {
        Some(Self::data_dir()?.join("log.json"))
    }

    fn input_history_path() -> Option<PathBuf> {
        Some(Self::data_dir()?.join("input_history.json"))
    }

    /// Reload the tape + ↑/↓ history from disk (best-effort — a missing or
    /// corrupt file just leaves the vec empty, like `LogStore::load`).
    pub(crate) fn load_persisted(&mut self) {
        if let Some(entries) = Self::log_path()
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice::<Vec<LogEntry>>(&bytes).ok())
        {
            *self.entries.borrow_mut() = entries;
        }
        if let Some(history) = Self::input_history_path()
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice::<Vec<String>>(&bytes).ok())
        {
            self.history = history;
        }
    }

    /// Snapshot the whole (small) tape + input history to disk, capped to the
    /// newest [`LOG_LIMIT`](Self::LOG_LIMIT). A no-op for an ephemeral session.
    pub(crate) fn save_persisted(&self) {
        if !self.persists {
            return;
        }
        let entries = self.entries.borrow();
        let tape = &entries[entries.len().saturating_sub(Self::LOG_LIMIT)..];
        if let (Some(path), Ok(bytes)) = (Self::log_path(), serde_json::to_vec(tape)) {
            let _ = std::fs::write(path, bytes);
        }
        let history = &self.history[self.history.len().saturating_sub(Self::LOG_LIMIT)..];
        if let (Some(path), Ok(bytes)) = (Self::input_history_path(), serde_json::to_vec(history)) {
            let _ = std::fs::write(path, bytes);
        }
    }
}
