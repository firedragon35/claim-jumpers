//! Claim Jumpers gameplay simulation.
//!
//! A deterministic, rendering-free port of the browser game's rules
//! (src/js in the repository root, which remains the reference). The game
//! crate feeds it [`Input`] once per fixed step, calls [`World::step`], and
//! drains [`World::events`] for sounds, effects and radio lines.
//!
//! Numbers follow the browser's f64 arithmetic and update order closely
//! enough to match its recorded golden traces (tests/golden.rs).

pub mod cable;
pub mod career;
pub mod combat;
pub mod consts;
pub mod crabs;
pub mod entities;
pub mod events;
pub mod input;
pub mod items;
pub mod js;
pub mod mods;
pub mod opposition;
pub mod parts;
pub mod physics;
pub mod rivals;
pub mod rng;
pub mod world;

pub use events::Event;
pub use input::{Action, Input};
pub use world::World;
