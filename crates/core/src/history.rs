use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub timestamp: u64,
    pub region: String,
    pub original: String,
    pub translation: String,
}

#[derive(Default)]
pub struct History { pub entries: Vec<Entry> }

impl History {
    pub fn path() -> PathBuf { dirs::data_local_dir().unwrap_or_default().join("lipa/history.json") }
    pub fn load(path: &Path, limit: usize) -> Self {
        let entries = match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|e| {
                // The next save overwrites the file: keep the history that could not be read.
                let kept = crate::files::keep_copy(path, "broken");
                tracing::error!(error = %e, copy = ?kept, "Не удалось разобрать файл истории");
                Vec::new()
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => { tracing::error!(error = %e, "Не удалось прочитать историю"); Vec::new() },
        };
        let mut history: Self = Self { entries };
        history.trim(limit);
        history
    }
    pub fn trim(&mut self, limit: usize) {
        let excess = self.entries.len().saturating_sub(limit);
        self.entries.drain(..excess);
    }
    pub fn push(&mut self, entry: Entry, limit: usize) { self.entries.push(entry); self.trim(limit); }
    pub fn save(&self, path: &Path, persist: bool) -> std::io::Result<()> {
        if !persist {
            return match std::fs::remove_file(path) { Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()), other => other };
        }
        if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
        crate::files::write_atomic(path, &serde_json::to_vec(&self.entries)?, 0o600)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_history_file_that_cannot_be_read_is_kept_before_the_next_save_overwrites_it() {
        let dir = std::env::temp_dir().join(format!("lipa-history-broken-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("history.json");
        std::fs::write(&path, r#"[{"timestamp": 1, "region": "r", "original": "a"}]"#).unwrap();
        let loaded = History::load(&path, 10);
        assert!(loaded.entries.is_empty());
        loaded.save(&path, true).unwrap();
        let kept = std::fs::read_to_string(dir.join("history.json.broken.bak")).expect("a copy of the unreadable file");
        assert!(kept.contains("\"original\": \"a\""));
    }

    use super::*;
    #[test]
    fn limits_roundtrips_and_removes_saved_history() {
        let path = std::env::temp_dir().join(format!("lipa-history-test-{}.json", std::process::id()));
        let mut h = History::default();
        for timestamp in 0..4 { h.push(Entry { timestamp, region: "Диалог".into(), original: "hello".into(), translation: "привет".into() }, 2); }
        h.save(&path, true).unwrap();
        let loaded = History::load(&path, 2);
        assert_eq!(loaded.entries.len(), 2);
        assert_eq!(loaded.entries[0].timestamp, 2);
        assert_eq!(loaded.entries[0].translation, "привет");
        h.save(&path, false).unwrap();
        assert!(!path.exists());
    }
}
