//! Simulation step timings, a baseline for performance work (not a pass /
//! fail check: timings depend on the machine). Run with
//! `cargo test --release -p cj-sim --test bench -- --ignored --nocapture`
//! and record the numbers in docs/ with the machine they came from.

use cj_sim::career::{Career, Mode};
use cj_sim::consts::STEP;
use cj_sim::rng::Rng;
use cj_sim::{Input, World};
use std::time::Instant;

/// One simulated minute (3,600 steps) for 1, 8 and 20 pilots, with every
/// pilot thrusting and turning so the field stays busy.
#[test]
#[ignore = "a timing baseline, run by hand in release"]
fn a_minute_of_steps_for_1_8_and_20_pilots() {
    for pilots in [1usize, 8, 20] {
        let mut w = World::new_shift_for(Rng::seeded(86), (1280.0, 720.0), pilots);
        while w.pilots.len() < pilots {
            w.add_career_pilot(&Career::new(Mode::Standard));
        }
        let input = Input {
            thrust: 1.0,
            turn: 0.3,
            ..Input::default()
        };
        let inputs = vec![input; pilots];
        let t = Instant::now();
        for _ in 0..3_600 {
            w.step_all(&inputs, STEP);
        }
        let took = t.elapsed();
        println!(
            "{pilots:>2} pilots: 3600 steps in {:.1} ms ({:.3} ms per step)",
            took.as_secs_f64() * 1e3,
            took.as_secs_f64() * 1e3 / 3_600.0
        );
    }
}
