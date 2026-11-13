//! Whether the game window has the player's attention. miniquad reports a
//! focus change on Windows as "minimised" / "restored" (see its
//! `EventHandler::window_minimized_event`), and macroquad only passes those
//! on to an input subscriber, so this module is one.

use macroquad::input::utils::{register_input_subscriber, repeat_all_miniquad_input};
use macroquad::miniquad::EventHandler;

/// Tracks focus across frames.
pub struct Focus {
    sub: usize,
    /// The window is in the background (or minimised).
    pub away: bool,
}

/// What changed this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    None,
    Lost,
    Back,
}

/// Collects one frame's focus events.
#[derive(Default)]
struct Catch {
    lost: bool,
    back: bool,
}

impl EventHandler for Catch {
    fn update(&mut self) {}
    fn draw(&mut self) {}
    fn window_minimized_event(&mut self) {
        self.lost = true;
    }
    fn window_restored_event(&mut self) {
        self.back = true;
    }
}

impl Focus {
    pub fn new() -> Focus {
        Focus {
            sub: register_input_subscriber(),
            away: false,
        }
    }

    /// Reads this frame's window events. Must run every frame, or the
    /// subscriber's queue grows.
    pub fn update(&mut self) -> Change {
        let mut c = Catch::default();
        repeat_all_miniquad_input(&mut c, self.sub);
        // Both in one frame (a quick alt-tab and back): the later state is
        // unknown, so treat it as back, the harmless choice.
        let change = match (c.lost, c.back) {
            (_, true) if self.away => Change::Back,
            (true, false) if !self.away => Change::Lost,
            _ => Change::None,
        };
        match change {
            Change::Lost => self.away = true,
            Change::Back => self.away = false,
            Change::None => {}
        }
        change
    }
}
