//! Where a server keeps its players' careers (CJN-18): one file per player
//! in the server's own careers folder, never beside a single-player
//! `career.json`, so a player starts fresh on every server and their own
//! career is never touched.
//!
//! The files use the single-player career format and its rules: written
//! through a temporary file that is flushed before it is renamed (a crash
//! never leaves half a save), and loaded through `Career::from_json`, which
//! repairs what it can. A file that can't be loaded is copied aside as
//! `<player>.bad.json` before the fresh career replaces it.
//!
//! With no folder (tests, or one that can't be created) careers last as
//! long as the server runs.

use cj_net::PlayerId;
use cj_sim::career::Career;
use std::io::Write;
use std::path::{Path, PathBuf};

/// The careers on one server.
pub struct CareerStore {
    dir: Option<PathBuf>,
}

/// A player id as a file name: 32 hex digits, nothing a player can choose.
fn file_stem(id: &PlayerId) -> String {
    id.iter().map(|b| format!("{b:02x}")).collect()
}

/// Create `path`, write `text`, and wait until the disk has it.
fn write_flushed(path: &Path, text: &str) -> std::io::Result<()> {
    let mut file = std::fs::File::create(path)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()
}

impl CareerStore {
    /// Careers kept in `dir` (created if missing), or in memory only when
    /// `dir` is `None`. A folder that can't be created falls back to
    /// memory, with a sentence for the log.
    pub fn open(dir: Option<PathBuf>) -> (CareerStore, Option<String>) {
        match dir {
            None => (CareerStore { dir: None }, None),
            Some(d) => match std::fs::create_dir_all(&d) {
                Ok(()) => (CareerStore { dir: Some(d) }, None),
                Err(e) => (
                    CareerStore { dir: None },
                    Some(format!(
                        "can't use the careers folder {}: {e}; careers last until the server stops",
                        d.display()
                    )),
                ),
            },
        }
    }

    /// Player `id`'s saved career, if there is a usable one, and a line for
    /// the log if a file was there but couldn't be loaded (it is kept as
    /// `.bad.json`).
    pub fn load(&self, id: &PlayerId) -> (Option<Career>, Option<String>) {
        let Some(dir) = &self.dir else {
            return (None, None);
        };
        let stem = file_stem(id);
        let path = dir.join(format!("{stem}.json"));
        let career = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| Career::from_json(&text));
        if career.is_some() || !path.exists() {
            return (career, None);
        }
        let kept = std::fs::copy(&path, dir.join(format!("{stem}.bad.json")));
        let why = match kept {
            Ok(_) => format!("career {stem} couldn't be loaded; kept as {stem}.bad.json"),
            Err(e) => format!("career {stem} couldn't be loaded or kept aside: {e}"),
        };
        (None, Some(why))
    }

    /// Saves player `id`'s career; an error is a sentence for the log.
    pub fn save(&self, id: &PlayerId, c: &Career) -> Result<(), String> {
        let Some(dir) = &self.dir else {
            return Ok(());
        };
        let stem = file_stem(id);
        let tmp = dir.join(format!("{stem}.json.tmp"));
        write_flushed(&tmp, &c.to_json())
            .and_then(|()| std::fs::rename(&tmp, dir.join(format!("{stem}.json"))))
            .map_err(|e| format!("couldn't save career {stem}: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cj_sim::career::Mode;

    fn folder(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cj-careers-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    /// A saved career comes back as it was; another player has none.
    #[test]
    fn careers_round_trip_per_player() {
        let d = folder("trip");
        let (store, warn) = CareerStore::open(Some(d.clone()));
        assert!(warn.is_none());
        let mut c = Career::new(Mode::Hard);
        c.wallet = 1234.0;
        c.shift = 4;
        store.save(&[1; 16], &c).expect("saved");
        let (back, _) = store.load(&[1; 16]);
        assert_eq!(
            back.map(|b| (b.wallet, b.shift, b.mode)),
            Some((1234.0, 4, Mode::Hard))
        );
        assert!(store.load(&[2; 16]).0.is_none(), "a fresh start for others");
        assert!(!d.join(format!("{}.json.tmp", file_stem(&[1; 16]))).exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A damaged career file is no career, but it is kept aside.
    #[test]
    fn a_damaged_career_is_kept_aside() {
        let d = folder("bad");
        let (store, _) = CareerStore::open(Some(d.clone()));
        let stem = file_stem(&[3; 16]);
        std::fs::write(d.join(format!("{stem}.json")), "{not json").expect("written");
        let (c, why) = store.load(&[3; 16]);
        assert!(c.is_none());
        assert!(why.is_some_and(|w| w.contains("bad.json")));
        assert!(d.join(format!("{stem}.bad.json")).exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Without a folder nothing touches the disk.
    #[test]
    fn no_folder_means_memory_only() {
        let (store, _) = CareerStore::open(None);
        assert!(store.save(&[1; 16], &Career::new(Mode::Standard)).is_ok());
        assert!(store.load(&[1; 16]).0.is_none());
    }
}
