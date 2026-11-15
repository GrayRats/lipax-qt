//! Which program a window belongs to, by the process KWin reports for it.
//!
//! Games under Proton or Wine share window classes (`steam_app_default`, `wine`), and their titles may change (a score,
//! a level), so neither tells two games apart for good. The program does: Wine puts the Windows command line of the
//! game into the process's own command line (`Z:\games\CrossxMars\CrossxMars.exe`), while `/proc/<pid>/exe` points to
//! the Wine loader for every game alike. The name of the `.exe` is taken, lower case and without the path, so moving
//! the game to another folder keeps its profile.

/// The program of a command line as `/proc/<pid>/cmdline` holds it (arguments separated by NUL): the file name of
/// the first `.exe` among the first arguments (a Wine loader may come before it), else the file name of a native
/// program. `None` for an empty command line.
pub fn program_from_cmdline(cmdline: &[u8]) -> Option<String> {
    let args: Vec<String> = cmdline.split(|&b| b == 0).filter(|a| !a.is_empty()).map(|a| String::from_utf8_lossy(a).into_owned()).collect();
    let file_name = |arg: &str| -> String {
        // Windows and Unix separators alike; the arguments of a Windows command line may be quoted.
        let trimmed = arg.trim().trim_matches('"');
        trimmed.rsplit(['\\', '/']).next().unwrap_or(trimmed).trim().to_lowercase()
    };
    // Some launchers put the whole Windows command line into the first argument: cut it at ".exe".
    let exe_in = |arg: &str| -> Option<String> {
        let lower = arg.to_lowercase();
        let end = lower.find(".exe")? + 4;
        let name = file_name(&arg[..end]);
        (name.len() > 4).then_some(name)
    };
    if let Some(exe) = args.iter().take(3).find_map(|a| exe_in(a)) {
        return Some(exe);
    }
    args.first().map(|a| file_name(a)).filter(|n| !n.is_empty())
}

/// The program of the process `pid` (see [`program_from_cmdline`]); `None` if the process is gone or not readable.
pub fn program_of_pid(pid: u32) -> Option<String> {
    if pid == 0 { return None; }
    program_from_cmdline(&std::fs::read(format!("/proc/{pid}/cmdline")).ok()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_windows_program_of_a_wine_process_is_its_exe_without_the_path() {
        assert_eq!(program_from_cmdline(b"Z:\\home\\art\\Games\\Cross x Mars\\CrossxMars.exe\0").as_deref(), Some("crossxmars.exe"));
        assert_eq!(program_from_cmdline(b"C:\\Program Files\\Game\\Game.EXE\0-windowed\0").as_deref(), Some("game.exe"));
        // A loader in front of the game, or the whole Windows command line in one argument.
        assert_eq!(program_from_cmdline(b"/usr/bin/wine64-preloader\0Z:\\g\\Hold Me Tight.exe\0").as_deref(), Some("hold me tight.exe"));
        assert_eq!(program_from_cmdline(b"\"C:\\g\\Witcher3.exe\" -dx12\0").as_deref(), Some("witcher3.exe"));
        // A native program keeps the name of its binary.
        assert_eq!(program_from_cmdline(b"/usr/bin/supertuxkart\0--fullscreen\0").as_deref(), Some("supertuxkart"));
        assert_eq!(program_from_cmdline(b""), None);
        assert_eq!(program_from_cmdline(b"\0\0"), None);
    }

    #[test]
    fn the_program_of_this_very_process_is_read_from_proc() {
        let me = program_of_pid(std::process::id()).expect("the test process has a command line");
        assert!(!me.is_empty() && !me.contains('/'), "{me}");
        assert_eq!(program_of_pid(0), None);
    }
}
