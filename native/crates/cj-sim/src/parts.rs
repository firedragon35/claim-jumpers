//! The machine shop's catalogues (src/js/01-tuning-constants.js): parts
//! and the hardpoints they mount in, levelled tug and weapon systems, and
//! the tug classes a career trades up through.

use crate::consts::SHIP_MAX_HP;

/// A part bought in the shop (UPGRADES). Declaration order is the
/// catalogue's display order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Cannon,
    Twin,
    Winch,
    Thrusters,
    Scoop,
    Grinder,
    Armor,
    Missiles,
    DroneBay,
    HullSeal,
    Grapple,
}

/// The kind of hardpoint a part mounts in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotType {
    Weapon,
    /// The mount on the cutting cannon itself (the twin cutter).
    Mod,
    Utility,
}

impl SlotType {
    pub fn label(self) -> &'static str {
        match self {
            SlotType::Weapon => "weapon",
            SlotType::Mod => "mod",
            SlotType::Utility => "utility",
        }
    }
}

pub struct PartDef {
    /// Key in the browser's save file.
    pub key: &'static str,
    pub name: &'static str,
    pub slot: SlotType,
    pub price: f64,
    /// What it does, and what it costs you.
    pub does: &'static str,
    pub cost: &'static str,
    /// Never for sale: awarded by finishing a contract chain.
    pub unique: bool,
}

pub const PARTS: [Part; 11] = [
    Part::Cannon,
    Part::Twin,
    Part::Winch,
    Part::Thrusters,
    Part::Scoop,
    Part::Grinder,
    Part::Armor,
    Part::Missiles,
    Part::DroneBay,
    Part::HullSeal,
    Part::Grapple,
];

impl Part {
    pub fn def(self) -> &'static PartDef {
        use SlotType::*;
        const fn d(
            key: &'static str,
            name: &'static str,
            slot: SlotType,
            price: f64,
            does: &'static str,
            cost: &'static str,
            unique: bool,
        ) -> PartDef {
            PartDef {
                key,
                name,
                slot,
                price,
                does,
                cost,
                unique,
            }
        }
        const CANNON: PartDef = d(
            "cannon",
            "Cutting cannon",
            Weapon,
            0.0,
            "Breaks rocks, wrecks and rival crews.",
            "Builds heat when held down.",
            false,
        );
        const TWIN: PartDef = d("twin", "Twin cutter", Mod, 2400.0,
            "Bolts a double barrel onto the cutting cannon: every trigger pull is a three-shot volley. No hardpoint needed.",
            "More heat per volley. Does nothing without the cannon mounted.", false);
        const WINCH: PartDef = d(
            "winch",
            "Reinforced winch",
            Utility,
            2800.0,
            "Tow 26 t instead of 16, reel in faster, hook from farther away.",
            "Takes a utility hardpoint.",
            false,
        );
        const THRUSTERS: PartDef = d(
            "thrusters",
            "Side thrusters",
            Utility,
            2000.0,
            "Strafe sideways without changing heading (Z / C, right stick).",
            "Adds heat, and shares the cannon's heat sink.",
            false,
        );
        const SCOOP: PartDef = d(
            "scoop",
            "Magnetic scoop",
            Utility,
            1800.0,
            "Pulls in nearby scrap and ore and hooks it automatically.",
            "Also drags loose pebbles into your hull.",
            false,
        );
        const GRINDER: PartDef = d("grinder", "Salvage grinder", Utility, 1500.0,
            "Grind the last piece on the line into heavy slugs (G, gamepad X). Slugs fire instead of cutter rounds.",
            "Destroys its sale value. Grinding a reactor is a mistake.", false);
        const ARMOR: PartDef = d(
            "armor",
            "Armor plating",
            Utility,
            2200.0,
            "+50 hull.",
            "+1.5 t on the tug: slower to accelerate and to turn.",
            false,
        );
        const MISSILES: PartDef = d("missiles", "Auto-defense missiles", Weapon, 2600.0,
            "Fires homing missiles at hostiles near the tug on its own, and shoots down incoming nets.",
            "Takes a weapon hardpoint. Rack of 4, reloads one every 3 s. Ignores rocks and crabs.", false);
        const DRONE_BAY: PartDef = d("dronebay", "Drone bay", Utility, 2000.0,
            "Carries up to 3 sentry drones that orbit the tug and shoot hostiles. Buy sentries under Tug systems.",
            "Takes a utility hardpoint. Sentries destroyed during a shift are gone for good.", false);
        const HULL_SEAL: PartDef = d(
            "hullseal",
            "Self-sealing hull",
            Utility,
            0.0,
            "Slowly patches the hull in the field: +1.5 hull per second while you fly.",
            "Takes a utility hardpoint. Hollis Freight chain reward.",
            true,
        );
        const GRAPPLE: PartDef = d(
            "grapple",
            "Long-reach grapple",
            Utility,
            0.0,
            "Hooks cargo from 60% farther away.",
            "Takes a utility hardpoint. Salty Dog Salvage chain reward.",
            true,
        );
        match self {
            Part::Cannon => &CANNON,
            Part::Twin => &TWIN,
            Part::Winch => &WINCH,
            Part::Thrusters => &THRUSTERS,
            Part::Scoop => &SCOOP,
            Part::Grinder => &GRINDER,
            Part::Armor => &ARMOR,
            Part::Missiles => &MISSILES,
            Part::DroneBay => &DRONE_BAY,
            Part::HullSeal => &HULL_SEAL,
            Part::Grapple => &GRAPPLE,
        }
    }

    pub fn from_key(key: &str) -> Option<Part> {
        PARTS.into_iter().find(|p| p.def().key == key)
    }
}

/// A hardpoint on the tug (SLOTS).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    pub key: &'static str,
    pub kind: SlotType,
    pub label: &'static str,
}

pub const SLOTS: [Slot; 5] = [
    Slot {
        key: "w1",
        kind: SlotType::Weapon,
        label: "Weapon A",
    },
    Slot {
        key: "w2",
        kind: SlotType::Weapon,
        label: "Weapon B",
    },
    Slot {
        key: "mod",
        kind: SlotType::Mod,
        label: "Cannon mod",
    },
    Slot {
        key: "u1",
        kind: SlotType::Utility,
        label: "Utility A",
    },
    Slot {
        key: "u2",
        kind: SlotType::Utility,
        label: "Utility B",
    },
];

/// What is mounted in each of SLOTS, in order.
pub type Mounted = [Option<Part>; 5];

/// Levelled systems (SYSTEMS). Levels run 1..=max.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum System {
    Shield,
    Thrust,
    Armor,
    Cannon,
    Missiles,
    Drones,
}

pub const SYSTEMS: [System; 6] = [
    System::Shield,
    System::Thrust,
    System::Armor,
    System::Cannon,
    System::Missiles,
    System::Drones,
];

/// Price of going from level i+1 to i+2 (the fourth step only exists for
/// the five-level missile rack).
pub const SYSTEM_PRICES: [f64; 4] = [1500.0, 3000.0, 5000.0, 8000.0];
pub const SYSTEM_MAX: u8 = 4;

pub const SHIELD_CAP: [f64; 4] = [40.0, 60.0, 80.0, 100.0];
pub const SHIELD_REGEN: [f64; 4] = [3.0, 4.0, 5.0, 6.0];
pub const SHIELD_WAIT: [f64; 4] = [5.0, 4.5, 4.0, 3.5];
pub const ENGINE_THRUST: [f64; 4] = [1.0, 1.12, 1.24, 1.36];
pub const ENGINE_TURN: [f64; 4] = [1.0, 1.06, 1.12, 1.18];
pub const ENGINE_STRAFE: [f64; 4] = [1.0, 1.15, 1.3, 1.45];
pub const ENGINE_STRAFE_HEAT: [f64; 4] = [1.0, 0.85, 0.7, 0.55];
pub const ARMOR_HP: [f64; 4] = [0.0, 25.0, 50.0, 75.0];
pub const ARMOR_MASS: [f64; 4] = [0.0, 0.4, 0.8, 1.2];
pub const CANNON_DAMAGE: [f64; 4] = [1.0, 1.1, 1.2, 1.35];
pub const CANNON_SPREAD: [u32; 4] = [0, 2, 2, 4];
pub const CANNON_RATE: [f64; 4] = [1.0, 0.9, 0.8, 0.7];
pub const CANNON_HEAT: [f64; 4] = [1.0, 0.9, 0.8, 0.7];
pub const MISSILE_RACK_LV: [u32; 5] = [4, 5, 6, 8, 6];
pub const MISSILE_RELOAD_LV: [f64; 5] = [3.0, 2.6, 2.2, 1.8, 2.4];
pub const MISSILE_DAMAGE_LV: [f64; 5] = [2.5, 3.0, 3.0, 3.6, 9.0];
pub const MISSILE_SALVO_LV: [u32; 5] = [1, 2, 2, 3, 2];
pub const TORPEDO_SPLASH: f64 = 120.0;
pub const DRONE_HP_LV: [f64; 4] = [6.0, 8.0, 10.0, 13.0];
pub const DRONE_DAMAGE_LV: [f64; 4] = [0.6, 0.75, 0.9, 1.1];
pub const DRONE_FIRE_LV: [f64; 4] = [0.8, 0.7, 0.6, 0.5];

/// Sentry drones: at most this many, at this price each.
pub const SENTRY_MAX: u32 = 3;
pub const SENTRY_PRICE: f64 = 900.0;

/// The browser's `Math.round` on a percentage, for shop descriptions.
fn pct(x: f64) -> i64 {
    crate::js::round(x * 100.0) as i64
}

impl System {
    pub fn key(self) -> &'static str {
        match self {
            System::Shield => "shield",
            System::Thrust => "thrust",
            System::Armor => "armor",
            System::Cannon => "cannon",
            System::Missiles => "missiles",
            System::Drones => "drones",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            System::Shield => "Shield generator",
            System::Thrust => "Engines",
            System::Armor => "Hull armor",
            System::Cannon => "Cutting cannon",
            System::Missiles => "Missile rack",
            System::Drones => "Sentry firmware",
        }
    }

    /// Weapon systems sit in their own shop column and need their part.
    pub fn needs(self) -> Option<Part> {
        match self {
            System::Cannon => Some(Part::Cannon),
            System::Missiles => Some(Part::Missiles),
            System::Drones => Some(Part::DroneBay),
            _ => None,
        }
    }

    pub fn max(self) -> u8 {
        if self == System::Missiles {
            5
        } else {
            SYSTEM_MAX
        }
    }

    /// What the system does at level `lv` (SYSTEMS[k].describe).
    pub fn describe(self, lv: u8) -> String {
        let i = usize::from(lv.max(1) - 1);
        match self {
            System::Shield => format!(
                "{} shield, recharging {}/s after {} s",
                SHIELD_CAP[i], SHIELD_REGEN[i], SHIELD_WAIT[i]
            ),
            System::Thrust if lv == 1 => "Stock thrust and turning".to_owned(),
            System::Thrust => format!(
                "+{}% thrust, +{}% turning, stronger and cooler side thrusters",
                pct(ENGINE_THRUST[i] - 1.0),
                pct(ENGINE_TURN[i] - 1.0)
            ),
            System::Armor if lv == 1 => format!("Stock hull ({SHIP_MAX_HP})"),
            System::Armor => format!("+{} hull, +{} t", ARMOR_HP[i], ARMOR_MASS[i]),
            System::Cannon if lv == 1 => "Stock cutter".to_owned(),
            System::Cannon => format!(
                "+{} spread shots, +{}% damage, {}% faster, {}% less heat",
                CANNON_SPREAD[i],
                pct(CANNON_DAMAGE[i] - 1.0),
                pct(1.0 - CANNON_RATE[i]),
                pct(1.0 - CANNON_HEAT[i])
            ),
            System::Missiles if lv == 5 => format!(
                "Torpedoes: {} in the rack, {} per salvo, {} damage plus a {} px blast",
                MISSILE_RACK_LV[4], MISSILE_SALVO_LV[4], MISSILE_DAMAGE_LV[4], TORPEDO_SPLASH
            ),
            System::Missiles => format!(
                "{} missiles, {} per salvo, reload {} s, {} damage",
                MISSILE_RACK_LV[i], MISSILE_SALVO_LV[i], MISSILE_RELOAD_LV[i], MISSILE_DAMAGE_LV[i]
            ),
            System::Drones => format!(
                "Sentries: {} hull, {} damage every {} s",
                DRONE_HP_LV[i], DRONE_DAMAGE_LV[i], DRONE_FIRE_LV[i]
            ),
        }
    }
}

/// System levels, each 1..=its max.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Levels {
    pub shield: u8,
    pub thrust: u8,
    pub armor: u8,
    pub cannon: u8,
    pub missiles: u8,
    pub drones: u8,
}

impl Default for Levels {
    /// baseLevels(): everything at level 1.
    fn default() -> Levels {
        Levels {
            shield: 1,
            thrust: 1,
            armor: 1,
            cannon: 1,
            missiles: 1,
            drones: 1,
        }
    }
}

impl Levels {
    pub fn get(&self, s: System) -> u8 {
        match s {
            System::Shield => self.shield,
            System::Thrust => self.thrust,
            System::Armor => self.armor,
            System::Cannon => self.cannon,
            System::Missiles => self.missiles,
            System::Drones => self.drones,
        }
    }

    pub fn set(&mut self, s: System, v: u8) {
        let v = v.clamp(1, s.max());
        match s {
            System::Shield => self.shield = v,
            System::Thrust => self.thrust = v,
            System::Armor => self.armor = v,
            System::Cannon => self.cannon = v,
            System::Missiles => self.missiles = v,
            System::Drones => self.drones = v,
        }
    }

    /// Zero-based table index for a system, clamped to its range.
    pub fn idx(&self, s: System) -> usize {
        usize::from(self.get(s).clamp(1, s.max()) - 1)
    }
}

/// A tug class (TUG_CLASSES). Paying off a loan trades up to the next.
pub struct TugClass {
    pub name: &'static str,
    pub mk: &'static str,
    pub loan: f64,
    pub hp: f64,
    pub shield: f64,
    pub thrust: f64,
    pub turn: f64,
    pub winch: f64,
    /// Paint: hull fill and accent stripe.
    pub hull: u32,
    pub trim: u32,
}

/// The starting loan (LOAN_START).
pub const LOAN_START: f64 = 40000.0;

#[rustfmt::skip]
pub const TUG_CLASSES: [TugClass; 5] = [
    TugClass { name: "Second Mortgage", mk: "Mk I", loan: LOAN_START, hp: 0.0, shield: 0.0, thrust: 1.0, turn: 1.0, winch: 0.0, hull: 0x2a1d06, trim: 0xffb02e },
    TugClass { name: "Refinance", mk: "Mk II", loan: 60000.0, hp: 25.0, shield: 15.0, thrust: 1.08, turn: 1.03, winch: 4.0, hull: 0x0f2230, trim: 0x4fd8ff },
    TugClass { name: "Balloon Payment", mk: "Mk III", loan: 85000.0, hp: 50.0, shield: 30.0, thrust: 1.16, turn: 1.06, winch: 8.0, hull: 0x24102e, trim: 0xc78bff },
    TugClass { name: "Golden Parachute", mk: "Mk IV", loan: 115000.0, hp: 80.0, shield: 45.0, thrust: 1.25, turn: 1.1, winch: 12.0, hull: 0x2c260a, trim: 0xffe066 },
    TugClass { name: "Hostile Takeover", mk: "Mk V", loan: 150000.0, hp: 120.0, shield: 60.0, thrust: 1.35, turn: 1.15, winch: 16.0, hull: 0x2e0c0c, trim: 0xff5a4d },
];
