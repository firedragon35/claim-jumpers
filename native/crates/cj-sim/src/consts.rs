//! Tuning constants, copied from the browser's src/js/01-tuning-constants.js.
//! Units: pixels (world units), seconds, tonnes. Velocities in px/s,
//! accelerations in px/s². Angles in radians, 0 = +x, increasing clockwise
//! on screen (canvas y points down).

/// Fixed simulation step (the browser runs 60 steps per simulated second).
pub const STEP: f64 = 1.0 / 60.0;
/// At most this many steps run per rendered frame; the backlog is dropped
/// after a long stall instead of spiralling (19-boot.js frame()).
pub const MAX_STEPS_PER_FRAME: u32 = 5;
/// Longest real frame time fed to the accumulator, in seconds.
pub const MAX_FRAME_DT: f64 = 0.1;

pub const WORLD_W: f64 = 5200.0;
pub const WORLD_H: f64 = 5200.0;
/// Length of one shift, in seconds.
pub const SHIFT_SECONDS: f64 = 300.0;

pub const SHIP_MASS: f64 = 3.0;
pub const SHIP_RADIUS: f64 = 18.0;
/// Main engine force; acceleration is this divided by the tug's mass.
pub const SHIP_THRUST: f64 = 960.0;
/// Maximum turn rate with no load, rad/s.
pub const SHIP_TURN: f64 = 3.4;
pub const SHIP_MAX_HP: f64 = 100.0;
/// Seconds of respawn invulnerability (and the start-of-shift grace).
pub const SHIP_INVULN: f64 = 2.0;
/// Seconds between losing the tug and the replacement launching.
pub const RESPAWN_TIME: f64 = 3.0;
/// Credits charged when the tug is wrecked (before difficulty fees).
pub const DEDUCTIBLE: f64 = 500.0;

/// Base winch capacity in tonnes (the stock winch).
pub const WINCH_CAPACITY: f64 = 16.0;
/// Reel-in speed of the stock winch, px/s.
pub const WINCH_REEL_SPEED: f64 = 150.0;
/// How far past an item's edge the stock claw can grab it.
pub const HOOK_RANGE: f64 = 150.0;
/// Spacing added between linked bodies at full reel-in.
pub const LINK_SLACK: f64 = 46.0;
/// Rope solver passes per step; more passes let corrections run down a long train.
pub const CABLE_ITERATIONS: usize = 6;
/// Jerk (px/s of velocity change) above which the cable rattles audibly.
pub const RATTLE_JERK: f64 = 40.0;

/// Fraction of winch capacity above which the engine sputters.
pub const SPUTTER_LOAD: f64 = 0.6;
/// Chance that each sputter window cuts the engine out.
pub const SPUTTER_CHANCE: f64 = 0.28;
/// Engine force while sputtering, as a fraction of normal.
pub const SPUTTER_FORCE: f64 = 0.3;
/// Reverse thrust relative to forward thrust.
pub const REVERSE_THROTTLE: f64 = 0.45;

/// Velocity damping (1/s) for each body type in integrateAll.
pub const SHIP_DAMP: f64 = 0.22;
pub const ITEM_DAMP: f64 = 0.1;
pub const WRECK_DAMP: f64 = 0.05;
/// Rocks faster than this slowly calm down at STORM_DAMP; slower ones drift freely.
pub const STORM_SPEED: f64 = 120.0;
pub const STORM_DAMP: f64 = 0.15;
/// Soft wall: bodies within WALL_MARGIN of the edge are pushed back with a
/// spring of stiffness WALL_PUSH (1/s²), not bounced.
pub const WALL_MARGIN: f64 = 60.0;
pub const WALL_PUSH: f64 = 3.0;

/// Restitution for circle collisions.
pub const RESTITUTION: f64 = 0.45;
/// Relative impact speed below which the tug takes no collision damage.
pub const SHIP_IMPACT_SAFE: f64 = 130.0;
/// Hull damage per px/s of impact speed above the safe limit.
pub const SHIP_IMPACT_DAMAGE: f64 = 0.085;
/// Item impact power (speed × mass) per point of rock damage.
pub const ROCK_IMPACT_POWER: f64 = 650.0;
/// Item impact power per point of wreck damage.
pub const WRECK_IMPACT_POWER: f64 = 900.0;
/// Seconds a broken-off pebble lasts before crumbling: [min, max].
pub const PEBBLE_LIFE: [f64; 2] = [20.0, 35.0];

/// Rock sizes 1..=3: (radius, hit points).
pub fn rock_size(size: u8) -> (f64, f64) {
    match size {
        3 => (70.0, 7.0),
        2 => (38.0, 3.0),
        _ => (18.0, 1.0),
    }
}

/// The salvage carrier sits at the centre of the field.
pub const CARRIER_X: f64 = WORLD_W / 2.0;
pub const CARRIER_Y: f64 = WORLD_H / 2.0;
/// The bay intake ring, on the carrier's port side.
pub const BAY_X: f64 = CARRIER_X - 330.0;
pub const BAY_Y: f64 = CARRIER_Y;
pub const BAY_R: f64 = 120.0;
/// The bay's tractor reaches this many bay radii out.
pub const BAY_TRACTOR_REACH: f64 = 2.2;
/// Tractor acceleration at the ring, px/s², falling to 0 at the reach.
pub const BAY_TRACTOR_PULL: f64 = 260.0;
pub const BAY_REPAIR_RANGE: f64 = 180.0;
/// Hull points repaired per second inside BAY_REPAIR_RANGE.
pub const BAY_REPAIR_RATE: f64 = 14.0;
/// Shield recharge multiplier while parked in the bay.
pub const SHIELD_BAY_BOOST: f64 = 3.0;
/// Bankings closer together than this (seconds) count as one delivery.
pub const DELIVERY_WINDOW: f64 = 2.0;

/// Static collision circles approximating the carrier hull: (dx from carrier centre, radius).
pub const CARRIER_HULL: [(f64, f64); 3] = [(-150.0, 75.0), (20.0, 82.0), (190.0, 66.0)];
/// Mass a static body counts as when the tug hits it.
pub const STATIC_IMPACT_MASS: f64 = 30.0;

/// Starting field: counts and the clearances kept around the carrier.
pub const START_WRECKS: usize = 9;
pub const START_ROCKS: usize = 95;
pub const START_ITEMS: usize = 10;
