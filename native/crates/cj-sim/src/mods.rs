//! The tug's performance numbers (computeMods in src/js/01-tuning-constants.js).
//!
//! Milestone 1 has no shop, so only the starting loadout exists: the stock
//! Mk I tug with the cutting cannon mounted and every system at level 1.
//! The full parts, levels and tug-class derivation arrives with the career
//! and shop in milestone 3.

use crate::consts::*;

#[derive(Clone, Debug, PartialEq)]
pub struct Mods {
    /// Tonnes the winch can hold.
    pub winch_cap: f64,
    /// Reel-in speed, px/s.
    pub reel: f64,
    /// Claw reach past an item's edge, px.
    pub hook_range: f64,
    pub max_hp: f64,
    pub mass: f64,
    /// Extra "cargo" counted for turning (armor plating counts as 2 t).
    pub turn_load: f64,
    pub thrust_mul: f64,
    pub turn_mul: f64,
    pub shield_max: f64,
    /// Shield points per second while recharging.
    pub shield_rate: f64,
    /// Seconds after a hit before the shield recharges.
    pub shield_delay: f64,
}

impl Mods {
    /// The new-career loadout: Mk I hull (class 0), cannon only, levels 1.
    /// Values are computeMods({w1: 'cannon'}, baseLevels(), 0).
    pub fn stock() -> Mods {
        Mods {
            winch_cap: WINCH_CAPACITY,
            reel: WINCH_REEL_SPEED,
            hook_range: HOOK_RANGE,
            max_hp: SHIP_MAX_HP,
            mass: SHIP_MASS,
            turn_load: 0.0,
            thrust_mul: 1.0,
            turn_mul: 1.0,
            // SHIELD_CAP[0], SHIELD_REGEN[0], SHIELD_WAIT[0].
            shield_max: 40.0,
            shield_rate: 3.0,
            shield_delay: 5.0,
        }
    }
}
