//! Adaptive music (src/js/07-adaptive-music.js): a step sequencer on the
//! synth's music bus. A minor-key loop of bass and pads always plays; drums
//! come in as danger rises, and an arpeggio lead when things are really
//! bad. Intensity is read from the shift each frame and eased so the layers
//! don't flicker. Steps are scheduled 0.2 s ahead with per-note delays.

use crate::audio::{Audio, Wave};
use cj_sim::consts::rival_stage_index;
use cj_sim::World;

const MUSIC_VOLUME: f32 = 0.45;
const MUSIC_BPM: f64 = 112.0;
/// One 16th note, in seconds.
const MUSIC_STEP: f64 = 60.0 / MUSIC_BPM / 4.0;
/// Am - F - C - G, one bar (16 steps) each, in Hz.
const MUSIC_CHORDS: [[f32; 3]; 4] = [
    [110.0, 130.81, 164.81],
    [87.31, 110.0, 130.81],
    [130.81, 164.81, 196.0],
    [98.0, 123.47, 146.83],
];

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
    pub enabled: bool,
}

impl Music {
    pub fn new(enabled: bool) -> Music {
        Music {
            step: 0,
            next: 0.0,
            intensity: 0.0,
            started: false,
            enabled,
        }
    }

    /// Danger as the music hears it (0..1, before easing).
    fn target(w: &World) -> f32 {
        let s = &w.ship;
        let boss = w.jumpers.iter().any(|j| j.boss.is_some() && !j.dead);
        let t = 0.15
            + if w.under_attack_t > 0.0 { 0.4 } else { 0.0 }
            + f64::from(w.wanted) * 0.08
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
        let target = if scene == Scene::Menu {
            0.0
        } else {
            Self::target(w)
        };
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
            self.play_step(audio, (self.next - now) as f32);
            self.next += MUSIC_STEP;
            self.step += 1;
        }
    }

    fn play_step(&self, a: &Audio, t: f32) {
        let x = self.intensity;
        let i = self.step;
        let chord = MUSIC_CHORDS[((i / 16) % MUSIC_CHORDS.len() as u64) as usize];
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
            if s16 % 8 == 0 {
                a.kick(t);
            }
            if s16 % 2 == 0 {
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
}
