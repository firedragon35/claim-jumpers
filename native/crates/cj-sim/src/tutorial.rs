//! The guided first shift (src/js/11-guided-first-shift.js). Marge walks
//! a new pilot through one mechanic at a time; each step completes on an
//! observable action. While it runs, rivals stay away and the shift's
//! field event waits. Whether training has been done is remembered by the
//! game, which starts it and is told when it ends.

use crate::consts::{BAY_X, BAY_Y};
use crate::events::{Radio, Sfx};
use crate::js;
use crate::world::World;

/// The step being taught, and how long its action has held true.
#[derive(Clone, Debug, PartialEq)]
pub struct Tutorial {
    pub step: usize,
    pub hold_t: f64,
}

/// Each step's card text and the shorter line Marge says.
pub const TUTORIAL: [(&str, &str); 5] = [
    (
        "Hold W (or RT) to thrust and A / D to turn. Fly clear of the carrier bay.",
        "Thrust and turn. Get clear of the bay.",
    ),
    (
        "Fly up to a piece of salvage and press E (gamepad Y) to hook it onto your cable.",
        "Hook a piece of salvage.",
    ),
    (
        "Tow it home: drag the cargo into the bay ring to bank it. Heavier cargo swings wider.",
        "Tow it into the bay to bank it.",
    ),
    (
        "Press Space (gamepad RB) to fire the cutting cannon. Rocks break into smaller pieces, and some hold ore.",
        "Fire the cannon. Break a rock.",
    ),
    (
        "Cargo is a weapon. Hook something, swing hard, and press X (gamepad LB) to cut it loose. It flies.",
        "Hook something, swing, and cut it loose.",
    ),
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
        let (text, say) = TUTORIAL[t.step];
        self.radio_say(
            Radio::Training,
            true,
            false,
            text.to_owned(),
            say.to_owned(),
        );
    }

    fn tutorial_step_done(&self, step: usize) -> bool {
        match step {
            0 => js::dist(self.ship.b.x, self.ship.b.y, BAY_X, BAY_Y) > 450.0,
            1 => !self.chain.is_empty(),
            2 => self.stats.items > 0,
            3 => self.shots_fired >= 6,
            _ => self.threw_cargo,
        }
    }

    /// Advance when the current step's action has happened (after a beat).
    pub(crate) fn update_tutorial(&mut self, dt: f64) {
        let Some(step) = self.tut.as_ref().map(|t| t.step) else {
            return;
        };
        if !self.ship.alive {
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
        let (text, say) = if skipped {
            (
                "Fine. Skip the lesson. The rival crews will teach you instead.",
                "Fine. The rivals will teach you.",
            )
        } else {
            (
                "That is the job, hotshot. Bank enough to cover the installment, and watch for pink contacts. Rivals are inbound.",
                "That's the job. Rivals inbound.",
            )
        };
        self.radio_say(
            Radio::Training,
            true,
            false,
            text.to_owned(),
            say.to_owned(),
        );
    }
}
