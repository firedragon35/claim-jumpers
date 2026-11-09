//! Where the career, best shifts and chosen difficulty live on disk: the
//! browser keeps them in localStorage (claimjumpers.career, .best, .mode);
//! here they are files in %APPDATA%\ClaimJumpers, so they never depend on
//! the working directory or the install folder.
//!
//! `CLAIM_JUMPERS_DATA` overrides the folder (the smoke test uses it so a
//! check run never touches a player's career). If no folder can be used,
//! the game still plays and keeps the career for the session only, as the
//! browser does when storage is blocked.

use cj_sim::career::{Career, Mode, MODES};
use serde_json::{Map, Value};
use std::cell::RefCell;
use std::io::Write;
use std::path::{Path, PathBuf};

const CAREER: &str = "career.json";
/// A copy of a career file that could not be loaded, kept because the next
/// new career overwrites `CAREER`.
const CAREER_BAD: &str = "career.bad.json";
const BEST: &str = "best.json";
const MODE: &str = "mode.txt";
/// "done" once the guided first shift has been finished or skipped.
const TRAINING: &str = "training.txt";

pub struct Store {
    dir: Option<PathBuf>,
    /// The latest problem to show the player. The release build has no
    /// console, so printing an error alone would tell them nothing.
    error: RefCell<Option<String>>,
}

/// Best single-shift haul per difficulty mode.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Bests {
    pub by_mode: [Option<f64>; 3],
}

impl Bests {
    fn idx(m: Mode) -> usize {
        MODES.iter().position(|x| *x == m).expect("listed")
    }
    pub fn get(&self, m: Mode) -> Option<f64> {
        self.by_mode[Self::idx(m)]
    }
    pub fn set(&mut self, m: Mode, v: f64) {
        self.by_mode[Self::idx(m)] = Some(v);
    }
}

impl Store {
    pub fn open() -> Store {
        let dir = std::env::var_os("CLAIM_JUMPERS_DATA")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("APPDATA").map(|a| Path::new(&a).join("ClaimJumpers")))
            .filter(|d| std::fs::create_dir_all(d).is_ok());
        let error = dir
            .is_none()
            .then(|| "No save folder: this career lasts this session only.".to_owned());
        if let Some(e) = &error {
            eprintln!("{e}");
        }
        Store {
            dir,
            error: RefCell::new(error),
        }
    }

    /// Hand over the latest save problem once, for the on-screen toast.
    pub fn take_error(&self) -> Option<String> {
        self.error.borrow_mut().take()
    }

    /// Log a save problem and queue a short message for the player.
    fn report(&self, message: String, detail: impl std::fmt::Display) {
        eprintln!("{message} ({detail})");
        *self.error.borrow_mut() = Some(message);
    }

    pub fn folder(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    pub fn read(&self, name: &str) -> Option<String> {
        std::fs::read_to_string(self.dir.as_ref()?.join(name)).ok()
    }

    /// Write via a temporary file and a rename, so a crash mid-write never
    /// leaves a half-written save behind. The data is flushed to the disk
    /// before the rename: otherwise a power cut can leave the new name
    /// pointing at an empty file.
    pub fn write(&self, name: &str, text: &str) {
        let Some(dir) = &self.dir else { return };
        let tmp = dir.join(format!("{name}.tmp"));
        let res = write_flushed(&tmp, text).and_then(|_| std::fs::rename(&tmp, dir.join(name)));
        if let Err(e) = res {
            self.report(format!("Could not save {name}. Progress may be lost."), e);
        }
    }

    /// The saved career, or None if there isn't a usable one. A file that
    /// exists but can't be read or parsed is copied aside first, because
    /// starting a new career replaces it.
    pub fn load_career(&self) -> Option<Career> {
        let dir = self.dir.as_ref()?;
        let path = dir.join(CAREER);
        let career = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| Career::from_json(&text));
        if career.is_none() && path.exists() {
            if let Err(e) = std::fs::copy(&path, dir.join(CAREER_BAD)) {
                self.report(
                    "Your career file could not be read or backed up.".to_owned(),
                    e,
                );
            }
        }
        career
    }

    pub fn save_career(&self, c: &Career) {
        self.write(CAREER, &c.to_json());
    }

    pub fn clear_career(&self) {
        let Some(dir) = &self.dir else { return };
        match std::fs::remove_file(dir.join(CAREER)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                self.report("Could not clear the old career file.".to_owned(), e);
            }
            _ => {}
        }
    }

    /// Bests, stored as JSON by mode key. An older plain number (from
    /// before modes existed) reads as the Standard best.
    pub fn load_bests(&self) -> Bests {
        let mut b = Bests::default();
        let Some(raw) = self
            .read(BEST)
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        else {
            return b;
        };
        match raw {
            Value::Number(n) => {
                if let Some(v) = n.as_f64().filter(|v| v.is_finite()) {
                    b.set(Mode::Standard, v);
                }
            }
            Value::Object(o) => {
                for m in MODES {
                    if let Some(v) = o
                        .get(m.def().key)
                        .and_then(Value::as_f64)
                        .filter(|v| v.is_finite())
                    {
                        b.set(m, v);
                    }
                }
            }
            _ => {}
        }
        b
    }

    pub fn save_bests(&self, b: &Bests) {
        let o: Map<String, Value> = MODES
            .iter()
            .filter_map(|m| b.get(*m).map(|v| (m.def().key.to_owned(), Value::from(v))))
            .collect();
        self.write(BEST, &Value::Object(o).to_string());
    }

    /// The difficulty picked for the next new career (a convenience).
    pub fn load_mode(&self) -> Mode {
        self.read(MODE)
            .and_then(|t| Mode::from_key(t.trim()))
            .unwrap_or(Mode::Standard)
    }

    pub fn save_mode(&self, m: Mode) {
        self.write(MODE, m.def().key);
    }

    pub fn training_done(&self) -> bool {
        self.read(TRAINING).is_some_and(|t| t.trim() == "done")
    }

    /// tutorialPending: training was asked for again, or a career's first
    /// shift has never had it.
    pub fn training_pending(&self, shift: i32) -> bool {
        match self.read(TRAINING).as_deref().map(str::trim) {
            Some("replay") => true,
            Some("done") => false,
            _ => shift == 1,
        }
    }

    /// "Replay training": run it again on the next shift.
    pub fn save_training_replay(&self) {
        self.write(TRAINING, "replay");
    }

    pub fn save_training_done(&self) {
        self.write(TRAINING, "done");
    }
}

/// Create `path`, write `text`, and wait until the disk has it.
fn write_flushed(path: &Path, text: &str) -> std::io::Result<()> {
    let mut file = std::fs::File::create(path)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store(tag: &str) -> Store {
        let dir = std::env::temp_dir().join(format!("cj-save-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Store {
            dir: Some(dir),
            error: RefCell::new(None),
        }
    }

    #[test]
    fn career_bests_and_mode_round_trip_on_disk() {
        let s = temp_store("rt");
        assert!(s.load_career().is_none());
        let mut c = Career::new(Mode::Hard);
        c.wallet = 1234.0;
        s.save_career(&c);
        assert_eq!(s.load_career(), Some(c));
        s.clear_career();
        assert!(s.load_career().is_none());

        let mut b = Bests::default();
        b.set(Mode::Relaxed, 900.0);
        s.save_bests(&b);
        assert_eq!(s.load_bests(), b);
        s.write(BEST, "4200");
        assert_eq!(s.load_bests().get(Mode::Standard), Some(4200.0));

        s.save_mode(Mode::Hard);
        assert_eq!(s.load_mode(), Mode::Hard);
        s.write(MODE, "nonsense");
        assert_eq!(s.load_mode(), Mode::Standard);
        let _ = std::fs::remove_dir_all(s.dir.unwrap());
    }

    #[test]
    fn a_corrupt_save_is_no_career_but_is_kept() {
        let s = temp_store("bad");
        s.write(CAREER, "{ not json");
        assert!(s.load_career().is_none());
        // The next new career overwrites career.json, so the copy must exist.
        assert_eq!(s.read(CAREER_BAD).as_deref(), Some("{ not json"));
        let _ = std::fs::remove_dir_all(s.dir.unwrap());
    }

    #[test]
    fn no_career_file_makes_no_backup() {
        let s = temp_store("none");
        assert!(s.load_career().is_none());
        assert!(s.read(CAREER_BAD).is_none());
        let _ = std::fs::remove_dir_all(s.dir.unwrap());
    }

    #[test]
    fn a_failed_save_is_reported_once() {
        let s = temp_store("fail");
        let dir = s.dir.clone().unwrap();
        // Writing into a folder that no longer exists fails on every system.
        std::fs::remove_dir_all(&dir).unwrap();
        s.write(CAREER, "{}");
        let error = s.take_error().expect("the failure is reported");
        assert!(error.contains(CAREER), "names the file: {error}");
        assert!(s.take_error().is_none(), "and only once");
    }
}
