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

// ---------------------------------------------------------------------
// Combat (milestone 2).
// ---------------------------------------------------------------------

/// Seconds between cannon shots at cannon level 1.
pub const FIRE_INTERVAL: f64 = 0.13;
pub const BULLET_SPEED: f64 = 950.0;
/// Seconds a cutter round lives.
pub const BULLET_LIFE: f64 = 0.85;
/// Grinder slugs: seconds between shots, speed, lifetime and damage.
pub const SLUG_INTERVAL: f64 = 0.28;
pub const SLUG_SPEED: f64 = 1050.0;
pub const SLUG_LIFE: f64 = 1.1;
pub const SLUG_DAMAGE: f64 = 4.0;
/// Side-thruster force, and heat per second of full strafe.
pub const STRAFE_FORCE: f64 = SHIP_THRUST * 0.7;
pub const HEAT_STRAFE: f64 = 45.0;
/// The magnetic scoop's reach, its pull on light salvage (at most
/// SCOOP_MAX_MASS tonnes) and on pebbles.
pub const SCOOP_RANGE: f64 = 230.0;
pub const SCOOP_PULL: f64 = 320.0;
pub const SCOOP_PEBBLE_PULL: f64 = 420.0;
pub const SCOOP_MAX_MASS: f64 = 1.5;
/// Distance ahead of the tug's centre at which rounds leave the muzzle.
pub const MUZZLE: f64 = 24.0;
/// Radians between angled spread shots (cannon levels 2 and up).
pub const CANNON_SPREAD_ANGLE: f64 = 0.09;
/// Heat per trigger pull of a single barrel; 100 overheats the guns.
pub const HEAT_PER_SHOT: f64 = 4.5;
/// Heat shed per second.
pub const HEAT_COOL: f64 = 35.0;
/// An overheated cannon unlocks once heat falls below this.
pub const OVERHEAT_RESUME: f64 = 35.0;

/// Rival bolts: speed, lifetime and firing range.
pub const EBULLET_SPEED: f64 = 520.0;
pub const EBULLET_LIFE: f64 = 1.6;
pub const JUMPER_FIRE_RANGE: f64 = 650.0;
/// Damage of a rival bolt.
pub const EBULLET_DAMAGE: f64 = 5.0;

/// The alarm for incoming fire sounds at most this often, seconds.
pub const ATTACK_ALARM_GAP: f64 = 6.0;
/// Seconds the UNDER ATTACK warning lingers after the last attack.
pub const ATTACK_HUD_HOLD: f64 = 3.0;

/// A reactor's blast radius and the hull damage at its centre.
pub const REACTOR_BLAST_RADIUS: f64 = 230.0;
pub const REACTOR_BLAST_DAMAGE: f64 = 45.0;
/// Seconds between a reactor priming and blowing, so chain reactions
/// ripple outward instead of recursing.
pub const REACTOR_FUSE: f64 = 0.05;
/// Closing speed at which volatile cargo primes on impact.
pub const VOLATILE_IMPACT: f64 = 400.0;
/// Shove a blast gives a body of mass 1 at its centre, px/s.
pub const BLAST_SHOVE: f64 = 420.0;

/// Per-step velocity change that flings a crawling crab.
pub const JERK_SHAKE: f64 = 120.0;
/// Crab crawl speed along the cable, px/s.
pub const CRAB_CRAWL_SPEED: f64 = 55.0;
/// Hull points per second a latched crab chews (under the shield).
pub const CRAB_HULL_DPS: f64 = 4.0;
/// Normalised spin meter above which the tug throws a latched crab.
pub const CRAB_SPIN_TO_SHAKE: f64 = 1.6;
/// Engine force while any crab is latched (they chew the thruster housings).
pub const CRAB_LATCHED_THRUST: f64 = 0.65;
pub const CRAB_RADIUS: f64 = 9.0;

pub const JUMPER_HP: f64 = 4.0;
/// A rival's pull while hauling, versus the tug's ~320 unloaded.
pub const JUMPER_HAUL_ACCEL: f64 = 210.0;
/// Seconds latched onto your train before a rival cuts your line.
pub const JUMPER_SEVER_TIME: f64 = 3.5;
/// How close hunters circle the tug.
pub const HUNTER_ORBIT: f64 = 230.0;
/// Rivals this close to a wingmate smashed by cargo flee for SCATTER_TIME.
pub const SCATTER_RANGE: f64 = 600.0;
pub const SCATTER_TIME: f64 = 4.0;
/// From Hostile on, rivals this close to a latched one give covering fire.
pub const COVER_RANGE: f64 = 900.0;
pub const COVER_TIME: f64 = 5.0;
pub const JUMPER_DAMP: f64 = 0.5;
/// A rival this far outside the field with your cargo has escaped.
pub const JUMPER_ESCAPE_MARGIN: f64 = 150.0;

/// Seconds before the first rival arrives on shift `shift`.
pub fn first_jumper_delay(shift: i32) -> f64 {
    (35.0 - 5.0 * f64::from(shift - 1)).max(12.0)
}
/// Most jumpers alive at once: 2, 2, 3, 3, 4, 4, then 5.
pub fn jumper_cap_for(shift: i32) -> f64 {
    (1.0 + (f64::from(shift) / 2.0).ceil()).min(5.0)
}
/// Seconds between jumper arrivals; shortens over the shift and across shifts.
pub fn jumper_interval_for(shift: i32, t: f64) -> f64 {
    (42.0 - t / 12.0).max(16.0) / (1.0 + 0.12 * f64::from(shift - 1))
}

/// Rival hull variants (JUMPER_VARIANTS). Each multiplies the base jumper.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variant {
    Skiff,
    Cutter,
    Brute,
    Gunship,
}

pub struct VariantDef {
    pub name: &'static str,
    pub r: f64,
    pub mass: f64,
    /// Extra hit points over JUMPER_HP.
    pub hp: f64,
    pub speed: f64,
    /// Multiplier on JUMPER_HAUL_ACCEL.
    pub haul: f64,
    /// Multiplier on the grudge stage's fire interval (lower is faster).
    pub fire: f64,
    /// Multiplier on JUMPER_SEVER_TIME.
    pub sever: f64,
    /// Bolts per volley.
    pub shots: u32,
    /// Hull fill and outline colours (0xRRGGBB); a `None` outline uses
    /// the palette's jumper colour.
    pub hull: u32,
    pub stroke: Option<u32>,
    pub bounty: f64,
    /// Spawn weight at each rival stage (Business..Vendetta).
    pub weights: [f64; 4],
    /// Always comes for the tug instead of the cargo.
    pub hunter: bool,
}

/// In the browser's declaration order, which the weighted pick walks.
pub const VARIANTS: [Variant; 4] = [
    Variant::Skiff,
    Variant::Cutter,
    Variant::Brute,
    Variant::Gunship,
];

impl Variant {
    pub fn def(self) -> &'static VariantDef {
        const SKIFF: VariantDef = VariantDef {
            name: "Skiff",
            r: 13.0,
            mass: 1.6,
            hp: -1.0,
            speed: 1.3,
            haul: 0.85,
            fire: 1.2,
            sever: 1.25,
            shots: 1,
            hull: 0x0d2421,
            stroke: Some(0x5ff0c8),
            bounty: 100.0,
            weights: [35.0, 30.0, 25.0, 20.0],
            hunter: false,
        };
        const CUTTER: VariantDef = VariantDef {
            name: "Cutter",
            r: 16.0,
            mass: 2.5,
            hp: 0.0,
            speed: 1.0,
            haul: 1.0,
            fire: 1.0,
            sever: 1.0,
            shots: 1,
            hull: 0x2a0d1d,
            stroke: None,
            bounty: 150.0,
            weights: [50.0, 45.0, 35.0, 30.0],
            hunter: false,
        };
        const BRUTE: VariantDef = VariantDef {
            name: "Brute",
            r: 21.0,
            mass: 4.5,
            hp: 3.0,
            speed: 0.8,
            haul: 1.45,
            fire: 1.4,
            sever: 0.75,
            shots: 1,
            hull: 0x2a1a08,
            stroke: Some(0xffae3d),
            bounty: 300.0,
            weights: [15.0, 18.0, 20.0, 20.0],
            hunter: false,
        };
        const GUNSHIP: VariantDef = VariantDef {
            name: "Gunship",
            r: 17.0,
            mass: 3.0,
            hp: 1.0,
            speed: 1.05,
            haul: 1.0,
            fire: 0.7,
            sever: 1.0,
            shots: 2,
            hull: 0x1c0d2a,
            stroke: Some(0xc78bff),
            bounty: 250.0,
            weights: [0.0, 7.0, 20.0, 30.0],
            hunter: true,
        };
        match self {
            Variant::Skiff => &SKIFF,
            Variant::Cutter => &CUTTER,
            Variant::Brute => &BRUTE,
            Variant::Gunship => &GUNSHIP,
        }
    }
}

/// Rival grudge stages (RIVAL_STAGES), each reached at grudge `at`.
pub struct RivalStage {
    pub name: &'static str,
    pub at: f64,
    /// Seconds between shots, random within the range.
    pub fire: (f64, f64),
    pub speed: f64,
    pub hp: f64,
    /// Share that come for the tug instead of the cargo.
    pub hunters: f64,
    /// Arrival-rate multiplier, and extra jumpers allowed at once.
    pub rate: f64,
    pub cap: f64,
    /// Whether they keep shooting while hauling your cargo away.
    pub haul_fire: bool,
    /// Whether they arrive in pairs.
    pub pack: bool,
}

#[rustfmt::skip]
pub const RIVAL_STAGES: [RivalStage; 4] = [
    RivalStage { name: "Business", at: 0.0, fire: (1.6, 2.4), speed: 1.0, hp: 0.0, hunters: 0.0, rate: 1.0, cap: 0.0, haul_fire: false, pack: false },
    RivalStage { name: "Annoyed", at: 2.0, fire: (1.2, 1.8), speed: 1.08, hp: 0.0, hunters: 0.0, rate: 1.1, cap: 0.0, haul_fire: true, pack: false },
    RivalStage { name: "Hostile", at: 5.0, fire: (0.9, 1.4), speed: 1.15, hp: 1.0, hunters: 0.35, rate: 1.25, cap: 1.0, haul_fire: true, pack: false },
    RivalStage { name: "Vendetta", at: 9.0, fire: (0.7, 1.1), speed: 1.22, hp: 2.0, hunters: 0.65, rate: 1.4, cap: 1.0, haul_fire: true, pack: true },
];

/// Index of the rivals' stage for a given grudge.
pub fn rival_stage_index(grudge: f64) -> usize {
    RIVAL_STAGES
        .iter()
        .enumerate()
        .fold(0, |idx, (i, st)| if grudge >= st.at { i } else { idx })
}

/// Field refills: a big rock drifts in every ROCK_REFILL_TIME seconds
/// while there are fewer than ROCK_REFILL_CAP; likewise wrecks.
pub const ROCK_REFILL_TIME: f64 = 6.0;
pub const ROCK_REFILL_CAP: usize = 90;
pub const WRECK_REFILL_TIME: f64 = 45.0;
pub const WRECK_REFILL_CAP: usize = 6;

/* Hit feedback (src/js/10d-presentation.js): real seconds of freeze or slow
 * motion the game applies to its fixed-step accumulator. */
pub const HITSTOP_SMASH: f64 = 0.08;
pub const HITSTOP_BOSS: f64 = 0.14;
pub const SLOWMO_BOSS: f64 = 1.2;
pub const SLOWMO_RATE: f64 = 0.35;

/// What Marge says was in a cracked sealed pod.
pub const POD_CONTENTS: [&str; 8] = [
    "medical supplies",
    "someone's vintage synthesizer",
    "400 kg of instant noodles",
    "unregistered jewellery",
    "a crate of very old whisky",
    "spare navigation chips",
    "a very confused cat (now the bay crew's cat)",
    "mining permits, some forged",
];
