use anyhow::Result;
use std::path::Path;

/// Information about the currently focused window.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ActiveApp {
    /// Lower-cased process name without extension (e.g. `code`, `cursor`, `warp`).
    pub process: String,
    /// Original window title as reported by the OS.
    pub title: String,
}

/// Sample the currently focused window. Returns `None` when no window is focused
/// or the OS query fails (e.g. permissions, headless session).
pub fn current_active_app() -> Result<Option<ActiveApp>> {
    let win = match active_win_pos_rs::get_active_window() {
        Ok(w) => w,
        Err(()) => {
            log::trace!("active-win-pos-rs returned no active window");
            return Ok(None);
        }
    };

    let process = process_name_from_path(&win.process_path);
    let title = win.title.trim().to_string();

    if process.is_empty() && title.is_empty() {
        return Ok(None);
    }

    Ok(Some(ActiveApp { process, title }))
}

/// Extract the binary name from a process path and normalize it
/// (lowercase, no extension). E.g. `C:\Program Files\Code.exe` → `code`.
fn process_name_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.trim().to_lowercase())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn extracts_windows_executable() {
        assert_eq!(
            process_name_from_path(&PathBuf::from(r"C:\Program Files\Microsoft VS Code\Code.exe")),
            "code"
        );
        assert_eq!(
            process_name_from_path(&PathBuf::from(r"C:\WindowsTerminal.exe")),
            "windowsterminal"
        );
    }

    #[test]
    fn extracts_unix_binaries() {
        assert_eq!(process_name_from_path(&PathBuf::from("/usr/bin/warp")), "warp");
        assert_eq!(
            process_name_from_path(&PathBuf::from("/Applications/Cursor.app/Contents/MacOS/Cursor")),
            "cursor"
        );
    }

    #[test]
    fn handles_empty_path() {
        assert_eq!(process_name_from_path(&PathBuf::from("")), "");
    }
}
