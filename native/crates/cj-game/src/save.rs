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
use std::path::{Path, PathBuf};

const CAREER: &str = "career.json";
const BEST: &str = "best.json";
const MODE: &str = "mode.txt";

pub struct Store {
    dir: Option<PathBuf>,
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
        if dir.is_none() {
            eprintln!("no writable save folder: the career lasts this session only");
        }
        Store { dir }
    }

    pub fn folder(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    fn read(&self, name: &str) -> Option<String> {
        std::fs::read_to_string(self.dir.as_ref()?.join(name)).ok()
    }

    /// Write via a temporary file and a rename, so a crash mid-write never
    /// leaves a half-written save behind.
    fn write(&self, name: &str, text: &str) {
        let Some(dir) = &self.dir else { return };
        let tmp = dir.join(format!("{name}.tmp"));
        let res = std::fs::write(&tmp, text).and_then(|_| std::fs::rename(&tmp, dir.join(name)));
        if let Err(e) = res {
            eprintln!("could not save {name}: {e}");
        }
    }

    pub fn load_career(&self) -> Option<Career> {
        Career::from_json(&self.read(CAREER)?)
    }

    pub fn save_career(&self, c: &Career) {
        self.write(CAREER, &c.to_json());
    }

    pub fn clear_career(&self) {
        if let Some(dir) = &self.dir {
            let _ = std::fs::remove_file(dir.join(CAREER));
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store(tag: &str) -> Store {
        let dir = std::env::temp_dir().join(format!("cj-save-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Store { dir: Some(dir) }
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
    fn a_corrupt_save_is_no_career() {
        let s = temp_store("bad");
        s.write(CAREER, "{ not json");
        assert!(s.load_career().is_none());
        let _ = std::fs::remove_dir_all(s.dir.unwrap());
    }
}
