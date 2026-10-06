//! Launch at login: an entry in the XDG autostart directory (`~/.config/autostart`), which KDE, GNOME and the other
//! desktops read at the start of the session. The entry is the same application the menu starts; it is created and
//! removed by the settings switch and nothing else touches it.

use std::path::{Path, PathBuf};

const FILE_NAME: &str = "io.lipa.Translator.desktop";

/// `~/.config/autostart` (or `$XDG_CONFIG_HOME/autostart`).
pub fn default_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("autostart"))
}

pub fn is_enabled(dir: &Path) -> bool {
    dir.join(FILE_NAME).is_file()
}

/// A path in the `Exec` key: the Desktop Entry spec wants a path with spaces or other reserved characters in double
/// quotes, with `"`, `` ` ``, `$` and `\` escaped (and the backslash escaped once more as a string value), and `%`
/// doubled so it is not taken for a field code.
pub fn exec_quote(path: &str) -> String {
    let path = path.replace('%', "%%");
    if !path.chars().any(|c| " \t\n\"'\\<>~|&;$*?#()`".contains(c)) { return path; }
    let mut quoted = String::from("\"");
    for c in path.chars() {
        if matches!(c, '"' | '`' | '$' | '\\') { quoted.push('\\'); }
        quoted.push(c);
    }
    quoted.push('"');
    quoted.replace('\\', "\\\\")
}

fn entry(exec: &str) -> String {
    format!("[Desktop Entry]\nType=Application\nName=LipaX\nComment=Перевод текста в играх\nExec={}\nIcon=io.lipa.Translator\nTerminal=false\nCategories=Utility;\nX-GNOME-Autostart-enabled=true\n", exec_quote(exec))
}

/// Create (`true`) or remove (`false`) the entry in `dir`. Creating writes through a temporary file, so a half-written
/// entry is never seen by the session; removing a missing entry is not an error.
pub fn set(dir: &Path, enabled: bool, exec: &str) -> std::io::Result<()> {
    let path = dir.join(FILE_NAME);
    if !enabled {
        return match std::fs::remove_file(&path) { Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e), _ => Ok(()) };
    }
    std::fs::create_dir_all(dir)?;
    let temporary = dir.join(format!(".{FILE_NAME}.tmp"));
    std::fs::write(&temporary, entry(exec))?;
    std::fs::rename(&temporary, &path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lipax-autostart-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn the_entry_is_created_and_removed() {
        let dir = temp("cycle").join("autostart");
        assert!(!is_enabled(&dir));
        set(&dir, true, "/usr/bin/lipax").unwrap();
        assert!(is_enabled(&dir));
        let text = std::fs::read_to_string(dir.join(FILE_NAME)).unwrap();
        assert!(text.contains("Exec=/usr/bin/lipax\n") && text.contains("Type=Application"), "{text}");
        assert!(!dir.join(format!(".{FILE_NAME}.tmp")).exists(), "no temporary file is left");
        set(&dir, true, "/usr/bin/lipax").unwrap();
        set(&dir, false, "/usr/bin/lipax").unwrap();
        assert!(!is_enabled(&dir));
        set(&dir, false, "/usr/bin/lipax").unwrap();
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }

    #[test]
    fn exec_paths_are_quoted_as_the_spec_asks() {
        assert_eq!(exec_quote("/usr/bin/lipax"), "/usr/bin/lipax");
        assert_eq!(exec_quote("/home/a b/lipax"), "\"/home/a b/lipax\"");
        assert_eq!(exec_quote("/opt/100%/lipax"), "/opt/100%%/lipax");
        assert_eq!(exec_quote("/x/$y"), "\"/x/\\\\$y\"");
    }
}
