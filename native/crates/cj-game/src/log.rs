//! The game's own log files in the save folder. The release exe has no
//! console (`windows_subsystem = "windows"`), so without these an audio
//! fault, a missing recording or a panic leaves no trace at all.
//!
//! - `game.log`: one line per problem, stamped with the Unix time; it starts
//!   again past 1 MB, keeping the last one as `game.old.log` (as the hosted
//!   server's `server.log` does).
//! - `crash.log`: written by the panic hook. On the next launch it is
//!   renamed `crash.last.log` and the player is told where it is.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The save folder, once known. Before [`init`] lines go to stderr only.
static DIR: OnceLock<PathBuf> = OnceLock::new();

/// Past this size `game.log` starts again.
const LOG_MAX: u64 = 1024 * 1024;
const LOG: &str = "game.log";
const CRASH: &str = "crash.log";
const CRASH_LAST: &str = "crash.last.log";

/// Points the log at the save folder and installs the panic hook. Returns
/// a sentence for the player if the previous run crashed.
pub fn init(dir: Option<&Path>) -> Option<String> {
    let dir = dir?;
    let _ = DIR.set(dir.to_path_buf());
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        write_crash(info);
        previous(info);
    }));
    let crash = dir.join(CRASH);
    if !crash.exists() {
        return None;
    }
    // Moved aside so the note is shown once, not on every launch.
    std::fs::rename(&crash, dir.join(CRASH_LAST)).ok()?;
    Some(format!(
        "The game closed unexpectedly last time. Details are in {CRASH_LAST} in {}.",
        dir.display()
    ))
}

/// Logs one problem: to stderr (seen in a dev build) and to `game.log`.
pub fn line(msg: &str) {
    eprintln!("{msg}");
    let Some(dir) = DIR.get() else { return };
    let path = dir.join(LOG);
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > LOG_MAX) {
        let _ = std::fs::rename(&path, dir.join("game.old.log"));
    }
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(f, "{} {msg}", unix_now());
    }
}

/// Seconds since 1970, for stamping lines.
fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The panic hook's report: what panicked and where, the version, and a
/// backtrace. Errors are ignored: a failing hook must not panic again.
fn write_crash(info: &std::panic::PanicHookInfo<'_>) {
    let Some(dir) = DIR.get() else { return };
    let text = format!(
        "Claim Jumpers {} crashed at {}\n{info}\n\n{}\n",
        env!("CARGO_PKG_VERSION"),
        unix_now(),
        std::backtrace::Backtrace::force_capture()
    );
    let _ = std::fs::write(dir.join(CRASH), text);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lines land in game.log once the folder is set. (DIR is process-wide,
    /// so this is the only test that sets it.)
    #[test]
    fn lines_reach_the_log_file() {
        let dir = std::env::temp_dir().join(format!("cj-log-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(CRASH), "old crash").unwrap();
        let note = init(Some(&dir)).expect("the old crash is reported");
        assert!(note.contains(CRASH_LAST));
        assert!(!dir.join(CRASH).exists() && dir.join(CRASH_LAST).exists());
        line("audio stream error: test");
        let text = std::fs::read_to_string(dir.join(LOG)).unwrap();
        // Other tests may log into the same file at the same time.
        assert!(text.contains(" audio stream error: test\n"), "{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
