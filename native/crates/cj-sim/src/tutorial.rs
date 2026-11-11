//! The guided first shift (src/js/11-guided-first-shift.js). Marge walks
//! Training is single-player only, so it always watches pilot 0.
//! a new pilot through one mechanic at a time; each step completes on an
//! observable action. While it runs, rivals stay away and the shift's
//! field event waits. Whether training has been done is remembered by the
//! game, which starts it and is told when it ends.

use crate::consts::{BAY_X, BAY_Y};
use crate::events::{Radio, Sfx};
use crate::js;
use crate::world::World;

/// The step being taught, and how long its action has held true.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Tutorial {
    pub step: usize,
    pub hold_t: f64,
}

/// Each step's card text, which Marge also reads out.
pub const TUTORIAL: [&str; 5] = [
    "Hold W (or RT) to thrust and A / D to turn. Fly clear of the carrier bay.",
    "Fly up to a piece of salvage and press E (gamepad Y) to hook it onto your cable.",
    "Tow it home: drag the cargo into the bay ring to bank it. Heavier cargo swings wider.",
    "Press Space (gamepad RB) to fire the cutting cannon. Rocks break into smaller pieces, and some hold ore.",
    "Cargo is a weapon. Hook something, swing hard, and press X (gamepad LB) to cut it loose. It flies.",
];

/// Seconds a step's action must hold before the next step (a short beat).
const TUTORIAL_BEAT: f64 = 0.8;

impl World {
    /// Starts the tutorial at its first step and shows it.
    pub fn start_tutorial(&mut self) {
        self.tut = Some(Tutorial {
            step: 0,
            hold_t: 0.0,
        });
        self.show_tutorial_step();
    }

    fn show_tutorial_step(&mut self) {
        let Some(t) = &self.tut else { return };
        let text = TUTORIAL[t.step];
        self.radio_text(Radio::Training, true, false, text.to_owned());
    }

    fn tutorial_step_done(&self, step: usize) -> bool {
        match step {
            0 => {
                js::dist(
                    self.pilots[0].ship.b.x,
                    self.pilots[0].ship.b.y,
                    BAY_X,
                    BAY_Y,
                ) > 450.0
            }
            1 => !self.pilots[0].chain.is_empty(),
            2 => self.pilots[0].stats.items > 0,
            3 => self.pilots[0].shots_fired >= 6,
            _ => self.pilots[0].threw_cargo,
        }
    }

    /// Advance when the current step's action has happened (after a beat).
    pub(crate) fn update_tutorial(&mut self, dt: f64) {
        let Some(step) = self.tut.as_ref().map(|t| t.step) else {
            return;
        };
        if !self.pilots[0].ship.alive {
            return;
        }
        let done = self.tutorial_step_done(step);
        let t = self.tut.as_mut().expect("checked");
        if !done {
            t.hold_t = 0.0;
            return;
        }
        t.hold_t += dt;
        if t.hold_t < TUTORIAL_BEAT {
            return;
        }
        t.hold_t = 0.0;
        t.step += 1;
        let finished = t.step >= TUTORIAL.len();
        self.sfx(Sfx::Bank);
        if finished {
            self.end_tutorial(false);
        } else {
            self.show_tutorial_step();
        }
    }

    /// Finish (or skip) training: the card goes and rivals come in.
    pub fn end_tutorial(&mut self, skipped: bool) {
        if self.tut.take().is_none() {
            return;
        }
        let text = if skipped {
            "Fine. Skip the lesson. The rival crews will teach you instead."
        } else {
            "That is the job, hotshot. Bank enough to cover the installment, and watch for pink contacts. Rivals are inbound."
        };
        self.radio_text(Radio::Training, true, false, text.to_owned());
    }
}
