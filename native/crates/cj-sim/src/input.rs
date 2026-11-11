//! Player input for one step (readInput in src/js/12-input.js), already
//! merged from keyboard and gamepad by the game layer.

/// Held controls, sampled once per simulation step.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Input {
    /// Forward throttle 0..1.
    pub thrust: f64,
    /// Brake / reverse 0..1.
    pub reverse: f64,
    /// Turn -1 (left, counter-clockwise on screen) .. 1 (right).
    pub turn: f64,
    /// Side thrusters -1..1 (needs the side thrusters part).
    pub strafe: f64,
    pub fire: bool,
    /// Afterburner (both triggers or Shift).
    pub burn: bool,
}

/// One-shot cable commands, triggered on a key or button press.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Action {
    /// Hook the nearest free item onto the end of the chain.
    Hook,
    /// Drop (throw) the last piece on the chain.
    Drop,
    /// Cut the whole chain loose.
    Cut,
    /// Grind the last piece into slugs (needs the salvage grinder).
    Grind,
    /// Switch PvP on or off, in the bay on a server that allows it (see
    /// [`crate::perm`]). Not a cable command, but sent the same way.
    TogglePvp,
}
