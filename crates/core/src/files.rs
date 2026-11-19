//! Writing the files of the program safely: whole or not at all, and no loss of a file the program could not read.

use std::io::Write;
use std::path::{Path, PathBuf};

/// Writes `bytes` to `path` through a temporary file beside it: the temporary file is created with `mode` (so a
/// secret is never readable by others, not even for a moment), flushed to the disk and renamed over `path`. A reader
/// sees the old file or the new one, never a half-written one; after a crash the old file is still there.
pub fn write_atomic(path: &Path, bytes: &[u8], mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut name = path.file_name().unwrap_or_default().to_owned();
    name.push(".tmp");
    let temporary = path.with_file_name(name);
    let written = (|| {
        let mut file = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(mode).open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()
    })();
    match written.and_then(|()| std::fs::rename(&temporary, path)) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&temporary);
            Err(e)
        }
    }
}

/// Keeps a copy of a file the program cannot understand (`reason` names it in the file name), because the next save
/// would overwrite it: a typo in a hand-edited file must not cost the API keys and profiles it holds. The copy is
/// `<file>.<reason>.bak`; an earlier, different copy is not replaced (the new one gets the time in its name).
/// The path of the copy; `None` if the file cannot be read or copied.
pub fn keep_copy(path: &Path, reason: &str) -> Option<PathBuf> {
    let content = std::fs::read(path).ok()?;
    let mut name = path.file_name()?.to_owned();
    name.push(format!(".{reason}.bak"));
    let mut backup = path.with_file_name(name);
    if let Ok(old) = std::fs::read(&backup) {
        if old == content { return Some(backup); }
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let mut name = path.file_name()?.to_owned();
        name.push(format!(".{reason}-{now}.bak"));
        backup = path.with_file_name(name);
    }
    write_atomic(&backup, &content, 0o600).ok()?;
    Some(backup)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lipa-files-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn an_atomic_write_replaces_the_file_keeps_the_mode_and_leaves_no_temporary_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = dir("atomic");
        let path = dir.join("config.toml");
        write_atomic(&path, b"one", 0o600).unwrap();
        write_atomic(&path, b"two", 0o600).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"two");
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1, "the temporary file is gone");
        // A directory in the way: the write fails and leaves nothing behind.
        let blocked = dir.join("blocked");
        std::fs::create_dir(&blocked).unwrap();
        assert!(write_atomic(&blocked, b"x", 0o600).is_err());
        assert!(!dir.join("blocked.tmp").exists());
    }

    #[test]
    fn a_copy_of_an_unreadable_file_is_kept_once_and_a_different_one_is_kept_beside_it() {
        let dir = dir("copy");
        let path = dir.join("config.toml");
        std::fs::write(&path, "history_limit = \"200\"").unwrap();
        let first = keep_copy(&path, "broken").expect("copied");
        assert_eq!(first, dir.join("config.toml.broken.bak"));
        assert_eq!(std::fs::read(&first).unwrap(), b"history_limit = \"200\"");
        assert_eq!(keep_copy(&path, "broken"), Some(first.clone()), "the same content is not copied again");
        std::fs::write(&path, "[capture\n").unwrap();
        let second = keep_copy(&path, "broken").expect("copied");
        assert_ne!(second, first);
        assert_eq!(std::fs::read(&first).unwrap(), b"history_limit = \"200\"", "the first copy is untouched");
        assert_eq!(std::fs::read(&second).unwrap(), b"[capture\n");
        assert_eq!(keep_copy(&dir.join("missing"), "broken"), None);
    }
}
