//! Adaptive music (src/js/07-adaptive-music.js): a step sequencer on the
//! synth's music bus. A minor-key loop of bass and pads always plays; drums
//! come in as danger rises, and an arpeggio lead when things are really
//! bad. Intensity is read from the shift each frame and eased so the layers
//! don't flicker. Steps are scheduled 0.2 s ahead with per-note delays.
//!
//! So a long shift doesn't wear one loop thin, the chords move through
//! several progressions, eight bars each, and every belt plays in its own
//! key. The menus have a calmer theme of their own, and short stings mark
//! banking, a jackpot, a wanted star and a boss arriving.

use crate::audio::{Audio, Wave};
use cj_sim::consts::rival_stage_index;
use cj_sim::field::Belt;
use cj_sim::World;

const MUSIC_VOLUME: f32 = 0.45;
const MUSIC_BPM: f64 = 112.0;
/// One 16th note, in seconds.
const MUSIC_STEP: f64 = 60.0 / MUSIC_BPM / 4.0;
/// The music's root, A2, in Hz; chords are semitones above it.
const ROOT: f32 = 110.0;

/// Minor-key progressions, four one-bar chords each, as semitones above
/// the root: i-VI-III-VII (the original Am F C G), i-iv-VI-v, i-VII-VI-VII
/// and i-III-iv-VI.
const PROGRESSIONS: [[[i8; 3]; 4]; 4] = [
    [[0, 3, 7], [-4, 0, 3], [3, 7, 10], [-2, 2, 5]],
    [[0, 3, 7], [5, 8, 12], [-4, 0, 3], [7, 10, 14]],
    [[0, 3, 7], [-2, 2, 5], [-4, 0, 3], [-2, 2, 5]],
    [[0, 3, 7], [3, 7, 10], [5, 8, 12], [-4, 0, 3]],
];
/// The order the progressions take turns in, eight bars each.
const PROGRESSION_ORDER: [usize; 6] = [0, 1, 0, 2, 3, 1];
/// Bars each progression plays before the next.
const BARS_PER_PROGRESSION: u64 = 8;

/// The menu theme's chords (Cmaj7, Am7, Fmaj7, G6), semitones above the root.
const MENU_CHORDS: [[i8; 4]; 4] = [[3, 7, 10, 14], [0, 3, 7, 10], [-4, 0, 3, 7], [-2, 2, 5, 9]];
/// The menu's bell melody: a semitone above the root on each of the bar's
/// four beats (`None` rests), one bar per chord.
const MENU_BELL: [[Option<i8>; 4]; 4] = [
    [Some(19), None, Some(17), Some(14)],
    [Some(15), None, None, Some(12)],
    [Some(15), Some(17), None, Some(12)],
    [Some(14), None, Some(10), None],
];

/// A short musical cue over the loop.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sting {
    /// A load banked: a quick rising major arpeggio.
    Bank,
    /// A jackpot: the same, an octave up and with a flourish.
    Jackpot,
    /// A wanted star: two falling notes, like a far-off siren.
    Wanted,
    /// A boss crew arriving: a low, slow minor second.
    Boss,
}

/// Where the music is heard from.
#[derive(Clone, Copy, PartialEq)]
pub enum Scene {
    Play,
    Paused,
    Menu,
}

pub struct Music {
    step: u64,
    /// Clock time (seconds) of the next step to schedule.
    next: f64,
    intensity: f32,
    started: bool,
    /// Semitones the field music is transposed by (the belt's key).
    key: i8,
    scene: Scene,
    pub enabled: bool,
}

/// A semitone above the root, in Hz.
fn hz(semis: f32) -> f32 {
    ROOT * 2f32.powf(semis / 12.0)
}

/// The key each belt plays in, in semitones from A: the quiet belts low
/// and settled, the dangerous ones higher and tenser.
pub fn belt_key(b: Belt) -> i8 {
    match b {
        Belt::Quiet => -2,
        Belt::Hollis => 0,
        Belt::Kessler => 1,
        Belt::Cinder => 3,
        Belt::Frontier => -4,
    }
}

impl Music {
    pub fn new(enabled: bool) -> Music {
        Music {
            step: 0,
            next: 0.0,
            intensity: 0.0,
            started: false,
            key: 0,
            scene: Scene::Menu,
            enabled,
        }
    }

    /// Danger as the music hears it (0..1, before easing).
    fn target(w: &World) -> f32 {
        let s = &w.me().ship;
        let boss = w.jumpers.iter().any(|j| j.boss.is_some() && !j.dead);
        let t =
            0.15 + if w.me().under_attack_t > 0.0 {
                0.4
            } else {
                0.0
            } + f64::from(w.me().wanted) * 0.08
                + rival_stage_index(w.grudge) as f64 * 0.08
                + if boss { 0.3 } else { 0.0 }
                + if s.net.is_some() { 0.35 } else { 0.0 }
                + if w.ion_t > 0.0 { 0.15 } else { 0.0 };
        t as f32
    }

    /// Ease the intensity, set the bus level, and schedule the coming steps.
    /// `now` is a steady clock in seconds; `dt` the frame time.
    pub fn update(&mut self, audio: &Audio, w: &World, scene: Scene, now: f64, dt: f32) {
        if !self.started {
            self.started = true;
            self.next = now + 0.1;
        }
        // Moving between the menus and the field starts the new theme on a
        // fresh bar.
        let menu = scene == Scene::Menu;
        if menu != (self.scene == Scene::Menu) {
            self.step = 0;
        }
        self.scene = scene;
        self.key = belt_key(w.belt);
        let target = if menu { 0.0 } else { Self::target(w) };
        // Rise quickly into danger, settle slowly after it.
        let k = if target > self.intensity { 2.0 } else { 0.35 };
        self.intensity += (target.min(1.0) - self.intensity) * (k * dt).min(1.0);
        let level = if !self.enabled {
            0.0
        } else {
            match scene {
                Scene::Play => 1.0,
                Scene::Paused => 0.35,
                Scene::Menu => 0.55,
            }
        };
        audio.set_music_gain(level * MUSIC_VOLUME);
        // A stall (window drag) drops the backlog instead of a burst of notes.
        if self.next < now {
            self.next = now;
        }
        while self.next < now + 0.2 {
            let t = (self.next - now) as f32;
            if menu {
                self.menu_step(audio, t);
            } else {
                self.play_step(audio, t);
            }
            self.next += MUSIC_STEP;
            self.step += 1;
        }
    }

    /// The field chord for step `i`: its progression turn, its bar, the
    /// belt's key.
    fn chord(&self, i: u64) -> [f32; 3] {
        let bar = i / 16;
        let turn = (bar / BARS_PER_PROGRESSION) as usize % PROGRESSION_ORDER.len();
        let prog = &PROGRESSIONS[PROGRESSION_ORDER[turn]];
        prog[(bar % 4) as usize].map(|s| hz(f32::from(s + self.key)))
    }

    fn play_step(&self, a: &Audio, t: f32) {
        let x = self.intensity;
        let i = self.step;
        let chord = self.chord(i);
        let s16 = i % 16;
        let step = MUSIC_STEP as f32;
        // Pads: the chord, once a bar, long and soft.
        if s16 == 0 {
            for f in chord {
                a.note(f * 2.0, Wave::Triangle, t, step * 16.0, 0.035, 1800.0);
            }
        }
        // Bass: a syncopated root that opens up with intensity.
        if [0, 3, 8, 11, 14].contains(&s16) {
            a.note(
                chord[0] / 2.0,
                Wave::Sawtooth,
                t,
                0.2,
                0.13,
                350.0 + x * 1400.0,
            );
        }
        // Drums from moderate danger.
        if x > 0.3 {
            if s16.is_multiple_of(8) {
                a.kick(t);
            }
            if s16.is_multiple_of(2) {
                a.music_noise(t, 0.04, 0.025 + x * 0.03, 7000.0);
            }
            if x > 0.55 && (s16 == 4 || s16 == 12) {
                a.music_noise(t, 0.12, 0.09, 1500.0);
            }
        }
        // Arpeggio lead when it's really hot.
        if x > 0.65 {
            a.note(
                chord[(s16 % 3) as usize] * 4.0,
                Wave::Square,
                t,
                step * 0.9,
                0.022,
                3200.0,
            );
        }
    }

    /// The menu theme: warm seventh-chord pads, a slow broken chord under
    /// them, and a bell melody on the beat. No bass line, no drums.
    fn menu_step(&self, a: &Audio, t: f32) {
        let i = self.step;
        let bar = ((i / 16) % 4) as usize;
        let chord = MENU_CHORDS[bar].map(|s| hz(f32::from(s)));
        let s16 = i % 16;
        let step = MUSIC_STEP as f32;
        if s16 == 0 {
            for f in chord {
                a.note(f, Wave::Triangle, t, step * 16.0, 0.03, 1200.0);
            }
        }
        if s16.is_multiple_of(4) {
            let f = chord[(s16 / 4) as usize % 4];
            a.note(f * 2.0, Wave::Sine, t, step * 3.5, 0.03, 0.0);
            if let Some(n) = MENU_BELL[bar][(s16 / 4) as usize] {
                a.note(
                    hz(f32::from(n)) * 2.0,
                    Wave::Sine,
                    t,
                    step * 6.0,
                    0.045,
                    0.0,
                );
            }
        }
    }

    /// Plays a sting over the loop, in the belt's key.
    pub fn sting(&self, a: &Audio, s: Sting) {
        if !self.enabled {
            return;
        }
        let k = f32::from(self.key);
        let notes: &[(f32, f32, f32)] = match s {
            // (semitones, start, length)
            Sting::Bank => &[(12.0, 0.0, 0.25), (16.0, 0.07, 0.25), (19.0, 0.14, 0.4)],
            Sting::Jackpot => &[
                (24.0, 0.0, 0.2),
                (28.0, 0.06, 0.2),
                (31.0, 0.12, 0.2),
                (36.0, 0.18, 0.6),
                (31.0, 0.3, 0.5),
            ],
            Sting::Wanted => &[(19.0, 0.0, 0.3), (15.0, 0.22, 0.45)],
            Sting::Boss => &[(-12.0, 0.0, 1.6), (-11.0, 0.5, 1.4), (0.0, 0.0, 1.8)],
        };
        let (wave, vol) = match s {
            Sting::Boss => (Wave::Sawtooth, 0.06),
            Sting::Wanted => (Wave::Square, 0.03),
            _ => (Wave::Triangle, 0.06),
        };
        for &(semis, at, len) in notes {
            a.note(hz(semis + k), wave, at, len, vol, 2400.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The field chords move to a new progression every eight bars, take
    /// every progression in turn, and follow the belt's key.
    #[test]
    fn progressions_take_turns_and_follow_the_belt_key() {
        let mut m = Music::new(true);
        let bar = |b: u64| b * 16;
        assert_eq!(m.chord(bar(0)), m.chord(bar(4)));
        // Bar 9 is the second progression's second chord, not the first's.
        assert_ne!(m.chord(bar(9)), m.chord(bar(1)));
        let used: std::collections::BTreeSet<usize> = PROGRESSION_ORDER.into_iter().collect();
        assert_eq!(used.len(), PROGRESSIONS.len());
        let home = m.chord(0)[0];
        m.key = belt_key(Belt::Cinder);
        let up = m.chord(0)[0];
        assert!(
            (up / home - 2f32.powf(3.0 / 12.0)).abs() < 1e-4,
            "{home} {up}"
        );
        // Every belt has a key within a few semitones of home.
        for b in cj_sim::field::BELTS {
            assert!(belt_key(b).abs() <= 6);
        }
    }

    /// The menu theme stays in its own chords; every field chord lies in a
    /// sensible range for the pads and bass.
    #[test]
    fn chords_stay_in_range() {
        let m = Music::new(true);
        for i in (0..16 * 48).step_by(16) {
            for f in m.chord(i) {
                assert!((60.0..400.0).contains(&f), "{f}");
            }
        }
        assert_eq!(hz(12.0), 220.0);
    }
}
