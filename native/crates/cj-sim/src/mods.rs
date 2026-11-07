//! The tug's performance numbers, derived from its mounted parts, system
//! levels and tug class (computeMods in src/js/01-tuning-constants.js).

use crate::consts::*;
use crate::parts::*;

#[derive(Clone, Debug, PartialEq)]
pub struct Mods {
    pub tug_class: usize,
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
    /// Sideways offsets of the cannon's barrels (empty with no cannon).
    pub barrels: Vec<f64>,
    /// Damage per cutter round.
    pub cannon_dmg: f64,
    /// Multipliers on the fire interval and on heat per shot.
    pub fire_mul: f64,
    pub heat_mul: f64,
    /// Extra angled shots per trigger pull (in pairs).
    pub cannon_spread: u32,
    // Utility parts.
    pub winch: bool,
    pub thrusters: bool,
    pub scoop: bool,
    pub grinder: bool,
    pub armor: bool,
    pub missiles: bool,
    pub drone_bay: bool,
    pub hull_seal: bool,
    /// Side-thruster force and heat multipliers (engine level).
    pub strafe_mul: f64,
    pub strafe_heat_mul: f64,
    // Missile rack and sentry firmware (used from milestone 4).
    pub missile_rack: u32,
    pub missile_reload: f64,
    pub missile_dmg: f64,
    pub missile_salvo: u32,
    pub torpedo: bool,
    pub drone_hp: f64,
    pub drone_dmg: f64,
    pub drone_fire: f64,
}

impl Mods {
    /// Number of barrels firing in parallel.
    pub fn guns(&self) -> usize {
        self.barrels.len()
    }

    /// The new-career loadout: Mk I hull, cannon only, every level at 1.
    pub fn stock() -> Mods {
        let mut mounted: Mounted = [None; 5];
        mounted[0] = Some(Part::Cannon);
        compute_mods(&mounted, &Levels::default(), 0)
    }
}

/// `computeMods(mounted, levels, tugClass)`.
///
/// Barrels: the cannon fires one centred shot; the twin cutter is a cannon
/// mod that adds two flanking barrels (a three-shot volley) and is inert
/// without the cannon underneath it.
pub fn compute_mods(mounted: &Mounted, levels: &Levels, tug_class: usize) -> Mods {
    let k = tug_class.min(TUG_CLASSES.len() - 1);
    let cls = &TUG_CLASSES[k];
    let has = |p: Part| mounted.contains(&Some(p));
    let lv = |s: System| levels.idx(s);
    let barrels = if has(Part::Cannon) {
        if has(Part::Twin) {
            vec![0.0, -7.0, 7.0]
        } else {
            vec![0.0]
        }
    } else {
        Vec::new()
    };
    let winch = has(Part::Winch);
    let armor = has(Part::Armor);
    let (ml, dl) = (lv(System::Missiles), lv(System::Drones));
    Mods {
        tug_class: k,
        barrels,
        winch_cap: (if winch {
            WINCH_CAPACITY + 10.0
        } else {
            WINCH_CAPACITY
        }) + cls.winch,
        reel: if winch { 230.0 } else { WINCH_REEL_SPEED },
        hook_range: (if winch { 195.0 } else { HOOK_RANGE })
            * if has(Part::Grapple) { 1.6 } else { 1.0 },
        winch,
        thrusters: has(Part::Thrusters),
        scoop: has(Part::Scoop),
        grinder: has(Part::Grinder),
        armor,
        missiles: has(Part::Missiles),
        drone_bay: has(Part::DroneBay),
        hull_seal: has(Part::HullSeal),
        max_hp: SHIP_MAX_HP + if armor { 50.0 } else { 0.0 } + ARMOR_HP[lv(System::Armor)] + cls.hp,
        mass: SHIP_MASS + if armor { 1.5 } else { 0.0 } + ARMOR_MASS[lv(System::Armor)],
        // Armor plating counts like 2 t of cargo for turning.
        turn_load: if armor { 2.0 } else { 0.0 },
        shield_max: SHIELD_CAP[lv(System::Shield)] + cls.shield,
        shield_rate: SHIELD_REGEN[lv(System::Shield)],
        shield_delay: SHIELD_WAIT[lv(System::Shield)],
        thrust_mul: ENGINE_THRUST[lv(System::Thrust)] * cls.thrust,
        turn_mul: ENGINE_TURN[lv(System::Thrust)] * cls.turn,
        strafe_mul: ENGINE_STRAFE[lv(System::Thrust)],
        strafe_heat_mul: ENGINE_STRAFE_HEAT[lv(System::Thrust)],
        cannon_dmg: CANNON_DAMAGE[lv(System::Cannon)],
        fire_mul: CANNON_RATE[lv(System::Cannon)],
        heat_mul: CANNON_HEAT[lv(System::Cannon)],
        cannon_spread: if has(Part::Cannon) {
            CANNON_SPREAD[lv(System::Cannon)]
        } else {
            0
        },
        missile_rack: MISSILE_RACK_LV[ml],
        missile_reload: MISSILE_RELOAD_LV[ml],
        missile_dmg: MISSILE_DAMAGE_LV[ml],
        missile_salvo: MISSILE_SALVO_LV[ml],
        torpedo: ml == 4,
        drone_hp: DRONE_HP_LV[dl],
        drone_dmg: DRONE_DAMAGE_LV[dl],
        drone_fire: DRONE_FIRE_LV[dl],
    }
}
