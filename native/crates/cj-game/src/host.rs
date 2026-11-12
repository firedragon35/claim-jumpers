//! Hosting a server inside the game (CJN-19).
//!
//! The server runs on its own thread with the same loop as the headless
//! `cj-server`, listening on every IPv4 interface on port 47820, and the
//! host's own game joins it over loopback like any other client, so the host
//! gets no privileged path. (The host still runs the server, and could
//! change anything in it.) When the host leaves, the server stops and every
//! player is disconnected; there is no host migration.

use std::net::{Ipv4Addr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

/// A server running inside the game.
pub struct Hosted {
    stop: Arc<AtomicBool>,
    /// What went wrong, if the server stopped after an error.
    failed: Arc<Mutex<Option<String>>>,
    thread: Option<JoinHandle<()>>,
}

impl Hosted {
    /// Starts a server for game version `version` and up to `limit` pilots
    /// (which also sizes the field) on port 47820, its careers in `mode`
    /// and kept in `careers` (in memory only if `None`), allowing PvP if
    /// `pvp`. Fails (with a
    /// sentence for the player) if the port is taken, for example by
    /// another server already running on this machine.
    pub fn start(
        version: &str,
        limit: usize,
        mode: cj_sim::career::Mode,
        pvp: bool,
        careers: Option<std::path::PathBuf>,
    ) -> Result<Hosted, String> {
        let port = cj_net::DEFAULT_PORT;
        let mut transport = cj_server::bind_udp(Ipv4Addr::UNSPECIFIED, port, limit)
            .map_err(|e| format!("Can't host on port {port}: {e}"))?;
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(1, |d| d.subsec_nanos());
        // The game has no console, so the server's log goes to a file in
        // its own folder (nothing secret is ever logged).
        let log_dir = careers.clone();
        let mut log = log_dir.as_deref().and_then(open_log);
        let mut server = cj_server::Server::new(cj_server::Config {
            version: version.to_owned(),
            max_players: limit,
            seed,
            record: false,
            mode,
            careers,
            pvp,
        });
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let failed = Arc::new(Mutex::new(None));
        let failure = failed.clone();
        let thread = std::thread::Builder::new()
            .name("cj-host-server".to_owned())
            .spawn(move || {
                let ran = cj_server::run(&mut transport, &mut server, |s| {
                    write_log(&mut log, log_dir.as_deref(), &mut s.log);
                    !flag.load(Ordering::Relaxed)
                });
                write_log(&mut log, log_dir.as_deref(), &mut server.log);
                if let (Err(why), Ok(mut f)) = (ran, failure.lock()) {
                    *f = Some(why);
                }
            })
            .map_err(|e| format!("Can't start the server: {e}"))?;
        Ok(Hosted {
            stop,
            failed,
            thread: Some(thread),
        })
    }

    /// Why the server stopped, if it stopped after an error.
    pub fn failed(&self) -> Option<String> {
        self.failed.lock().ok().and_then(|f| f.clone())
    }

    /// Stops the server, disconnecting everyone.
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            // A server thread that panicked has already stopped.
            let _ = t.join();
        }
    }
}

impl Drop for Hosted {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Past this size the log starts again, keeping the last one as
/// `server.old.log`: when hosting starts, and during a session too.
const LOG_MAX: u64 = 1024 * 1024;

/// The hosted server's log, `server.log` in its folder, opened to append.
/// None if it can't be opened: the server runs without a log then.
fn open_log(dir: &std::path::Path) -> Option<std::fs::File> {
    std::fs::create_dir_all(dir).ok()?;
    let path = dir.join("server.log");
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > LOG_MAX) {
        let _ = std::fs::rename(&path, dir.join("server.old.log"));
    }
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .ok()
}

/// Appends `lines` to the log, each stamped with the Unix time in seconds,
/// and empties them. A failed write drops the log rather than the server.
/// Once the log passes [`LOG_MAX`] it starts again in `dir`, the folder it
/// was opened in, so a long session (or a flood of refused clients) can't
/// grow it without limit.
fn write_log(
    log: &mut Option<std::fs::File>,
    dir: Option<&std::path::Path>,
    lines: &mut Vec<String>,
) {
    use std::io::Write;
    // Nearly every step logs nothing: skip the clock and the size check.
    if lines.is_empty() {
        return;
    }
    let Some(f) = log.as_mut() else {
        lines.clear();
        return;
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let ok = lines.drain(..).all(|l| writeln!(f, "{now} {l}").is_ok());
    if !ok {
        *log = None;
        return;
    }
    if f.metadata().is_ok_and(|m| m.len() > LOG_MAX) {
        // Closed before open_log renames it aside.
        *log = None;
        *log = dir.and_then(open_log);
    }
}

/// This machine's address on its local network, for telling other players
/// what to type. Connecting a UDP socket picks the interface that would
/// route to a far address without sending anything; the address used is in
/// a documentation range, so nothing is ever contacted.
pub fn lan_address() -> Option<Ipv4Addr> {
    let s = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    s.connect((Ipv4Addr::new(192, 0, 2, 1), 9)).ok()?;
    match s.local_addr().ok()?.ip() {
        std::net::IpAddr::V4(ip) if !ip.is_unspecified() && !ip.is_loopback() => Some(ip),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Log lines are appended with a time stamp, and the next start of a
    /// server that has filled its log keeps the old one aside.
    #[test]
    fn the_hosted_log_appends_and_starts_again_when_full() {
        let dir = std::env::temp_dir().join(format!("cj-host-log-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut log = open_log(&dir);
        let mut lines = vec!["Ann joined as pilot 1".to_owned()];
        write_log(&mut log, Some(&dir), &mut lines);
        assert!(lines.is_empty());
        drop(log);
        let text = std::fs::read_to_string(dir.join("server.log")).expect("a log");
        assert!(text.trim_end().ends_with(" Ann joined as pilot 1"));
        std::fs::write(dir.join("server.log"), vec![b'x'; LOG_MAX as usize + 1]).expect("filled");
        drop(open_log(&dir));
        assert!(dir.join("server.old.log").exists());
        assert_eq!(
            std::fs::metadata(dir.join("server.log"))
                .map(|m| m.len())
                .ok(),
            Some(0)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Regression: the log was only started again when hosting began, so a
    /// long session, or a flood of refused clients, grew it without limit.
    /// Past the limit mid-session it starts again, keeping the full one aside.
    #[test]
    fn the_hosted_log_starts_again_mid_session() {
        let dir = std::env::temp_dir().join(format!("cj-host-log-mid-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut log = open_log(&dir);
        let mut lines = vec!["x".repeat(LOG_MAX as usize + 1)];
        write_log(&mut log, Some(&dir), &mut lines);
        assert!(log.is_some(), "still logging");
        let old = std::fs::metadata(dir.join("server.old.log")).map(|m| m.len());
        assert!(old.is_ok_and(|n| n > LOG_MAX), "the full log is kept aside");
        let mut lines = vec!["Ann joined as pilot 1".to_owned()];
        write_log(&mut log, Some(&dir), &mut lines);
        drop(log);
        let text = std::fs::read_to_string(dir.join("server.log")).expect("a log");
        assert!(text.trim_end().ends_with(" Ann joined as pilot 1"));
        assert!(text.len() < 100, "a fresh log");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
