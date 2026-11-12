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
//! Beside each career, `<player>.key` holds a SHA-256 fingerprint of the
//! secret that first claimed it, so only that player can load it again,
//! even after the server restarts or has forgotten them. Only the
//! fingerprint is kept: each install uses one secret on every server it
//! joins, so a host who stored the secret itself could pass as that player
//! elsewhere.
//!
//! With no folder (tests, or one that can't be created) careers last as
//! long as the server runs.

use cj_net::PlayerId;
use cj_sim::career::Career;
use sha2::{Digest, Sha256};
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

    /// Whether `secret` may load player `id`'s career.
    ///
    /// The first secret to claim an id is bound to it: a new player, or a
    /// career saved before fingerprints were kept (1.4.2 and earlier), whose
    /// owner is the first to come back. Every disk fault refuses the claim,
    /// whether the fingerprint can't be read or can't be saved: a career
    /// must never be saved without its key, or the next secret to arrive
    /// would bind it. The caller refuses before the career is created, so a
    /// refused claim leaves nothing behind.
    pub fn claim(&self, id: &PlayerId, secret: &[u8; 16]) -> Result<(), Refusal> {
        let Some(dir) = &self.dir else {
            // In memory nothing outlives the player's own connection.
            return Ok(());
        };
        let stem = file_stem(id);
        let path = dir.join(format!("{stem}.key"));
        let print = fingerprint(secret);
        match std::fs::read_to_string(&path) {
            Ok(saved) if saved.trim() == print => Ok(()),
            Ok(_) => Err(Refusal::WrongSecret),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let tmp = dir.join(format!("{stem}.key.tmp"));
                write_flushed(&tmp, &print)
                    .and_then(|()| std::fs::rename(&tmp, &path))
                    .map_err(|e| {
                        // A half-written key is never read (only `.key` is),
                        // but leaving it would clutter the folder.
                        let _ = std::fs::remove_file(&tmp);
                        Refusal::Storage(format!("couldn't save the key for career {stem}: {e}"))
                    })
            }
            Err(e) => Err(Refusal::Storage(format!(
                "couldn't read the key for career {stem}: {e}"
            ))),
        }
    }
}

/// Why [`CareerStore::claim`] turned a secret away.
#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The career is bound to another secret.
    WrongSecret,
    /// The key couldn't be read or saved, so nobody can be let in safely;
    /// holds a sentence for the log.
    Storage(String),
}

/// A secret's SHA-256 as 64 hex digits. The secret is 16 random bytes, not
/// something a person chose, so a plain hash can't be guessed backwards and
/// a slow password hash would add nothing.
fn fingerprint(secret: &[u8; 16]) -> String {
    Sha256::digest(secret)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
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
        assert_eq!(store.claim(&[1; 16], &[9; 16]), Ok(()));
    }

    /// The first secret to claim a career is the only one that may load it
    /// after that; the file holds its fingerprint, never the secret.
    #[test]
    fn a_career_belongs_to_the_secret_that_first_claimed_it() {
        let d = folder("claim");
        let (store, _) = CareerStore::open(Some(d.clone()));
        let claim = |id: u8, secret: u8| store.claim(&[id; 16], &[secret; 16]);
        assert_eq!(claim(1, 7), Ok(()), "a new player binds their secret");
        assert_eq!(claim(1, 7), Ok(()), "and comes back with it");
        assert_eq!(claim(1, 8), Err(Refusal::WrongSecret), "another is refused");
        assert_eq!(claim(2, 8), Ok(()), "other players are their own");
        let saved =
            std::fs::read_to_string(d.join(format!("{}.key", file_stem(&[1; 16])))).expect("kept");
        assert_eq!(saved, fingerprint(&[7; 16]));
        assert_ne!(saved, file_stem(&[7; 16]), "not the secret itself");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A career saved before keys were kept goes to whoever claims it first,
    /// and is theirs from then on.
    #[test]
    fn an_older_career_is_bound_by_its_first_claim() {
        let d = folder("old");
        let (store, _) = CareerStore::open(Some(d.clone()));
        store
            .save(&[4; 16], &Career::new(Mode::Standard))
            .expect("saved");
        assert_eq!(store.claim(&[4; 16], &[5; 16]), Ok(()));
        assert_eq!(store.claim(&[4; 16], &[6; 16]), Err(Refusal::WrongSecret));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A key that is there but can't be read refuses the player rather than
    /// letting anyone in.
    #[test]
    fn an_unreadable_key_refuses_the_claim() {
        let d = folder("unreadable");
        let (store, _) = CareerStore::open(Some(d.clone()));
        // A folder where the key should be: it exists but can't be read.
        std::fs::create_dir_all(d.join(format!("{}.key", file_stem(&[3; 16])))).expect("made");
        let refused = store.claim(&[3; 16], &[3; 16]);
        assert!(matches!(refused, Err(Refusal::Storage(w)) if w.contains("couldn't read")));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Regression: a key that couldn't be saved still let the player in, so
    /// their career was saved with no key and the next secret to arrive
    /// could claim it. Now the claim is refused, no key is left behind, and
    /// nobody can claim the career until a key can be saved.
    #[test]
    fn a_key_that_cant_be_saved_refuses_the_claim() {
        let d = folder("unwritable");
        let (store, _) = CareerStore::open(Some(d.clone()));
        let stem = file_stem(&[5; 16]);
        // A folder where the key's temporary file goes: it can't be created.
        let tmp = d.join(format!("{stem}.key.tmp"));
        std::fs::create_dir_all(&tmp).expect("made");
        let refused = store.claim(&[5; 16], &[5; 16]);
        assert!(matches!(refused, Err(Refusal::Storage(w)) if w.contains("couldn't save")));
        assert!(!d.join(format!("{stem}.key")).exists(), "no key was bound");
        assert!(
            matches!(store.claim(&[5; 16], &[6; 16]), Err(Refusal::Storage(_))),
            "nor can another secret claim it meanwhile"
        );
        std::fs::remove_dir(&tmp).expect("cleared");
        assert_eq!(
            store.claim(&[5; 16], &[5; 16]),
            Ok(()),
            "bound once it can be saved"
        );
        assert_eq!(store.claim(&[5; 16], &[6; 16]), Err(Refusal::WrongSecret));
        let _ = std::fs::remove_dir_all(&d);
    }
}
