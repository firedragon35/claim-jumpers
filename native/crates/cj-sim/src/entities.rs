//! Bodies in the field and their factories (src/js/13-entity-factories.js).

use crate::consts::*;
use crate::items::{ItemDef, ItemType, Value};
use crate::js;
use crate::mods::Mods;
use crate::rng::Rng;
use std::f64::consts::{PI, TAU};

/// The physical part every colliding thing shares.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Body {
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
    pub r: f64,
    pub mass: f64,
    /// Static bodies (the carrier hull) never move; their inverse mass is 0.
    pub fixed: bool,
}

impl Body {
    /// Inverse mass, 0 for static bodies.
    pub fn inv_mass(&self) -> f64 {
        if self.fixed {
            0.0
        } else {
            1.0 / self.mass
        }
    }

    pub fn dist_to(&self, o: &Body) -> f64 {
        js::dist(self.x, self.y, o.x, o.y)
    }

    pub fn speed(&self) -> f64 {
        js::hypot(self.vx, self.vy)
    }
}

/// One vertex of an irregular outline: angle and radius multiplier.
#[derive(Clone, Copy, Debug)]
pub struct ShapePt {
    pub a: f64,
    pub m: f64,
}

/// Irregular polygon outline as radius multipliers around a circle.
pub fn jagged_shape(rng: &mut Rng, count: i32, lo: f64, hi: f64) -> Vec<ShapePt> {
    (0..count)
        .map(|i| ShapePt {
            a: (f64::from(i) / f64::from(count)) * TAU + rng.range(-0.15, 0.15),
            m: rng.range(lo, hi),
        })
        .collect()
}

/// The player's tug.
#[derive(Clone, Debug)]
pub struct Ship {
    pub b: Body,
    pub angle: f64,
    /// Angular velocity, rad/s.
    pub av: f64,
    pub hp: f64,
    pub alive: bool,
    pub respawn_t: f64,
    /// Seconds of invulnerability left (respawn grace).
    pub invuln: f64,
    /// Current throttle magnitude, for the exhaust flame.
    pub thrust_level: f64,
    /// Signed throttle this step: positive forward, negative reverse.
    pub throttle: f64,
    pub sputter_t: f64,
    pub sputtering: bool,
    /// How hard the tug has been spinning recently (crabs are thrown off by it).
    pub spin_meter: f64,
    pub low_warned: bool,
    /// Red hull flash after a hit, seconds.
    pub flash: f64,
    pub shield: f64,
    pub shield_delay_t: f64,
    pub shield_flash: f64,
}

impl Ship {
    /// A fresh tug at the bay, nose pointing away from the carrier.
    pub fn new(mods: &Mods) -> Ship {
        Ship {
            b: Body {
                x: BAY_X,
                y: BAY_Y,
                r: SHIP_RADIUS,
                mass: mods.mass,
                ..Body::default()
            },
            angle: PI,
            av: 0.0,
            hp: mods.max_hp,
            alive: true,
            respawn_t: 0.0,
            invuln: SHIP_INVULN,
            thrust_level: 0.0,
            throttle: 0.0,
            sputter_t: 0.0,
            sputtering: false,
            spin_meter: 0.0,
            low_warned: false,
            flash: 0.0,
            shield: mods.shield_max,
            shield_delay_t: 0.0,
            shield_flash: 0.0,
        }
    }

    /// The point on the stern where the tow cable leaves the tug.
    pub fn stern(&self) -> (f64, f64) {
        (
            self.b.x - self.angle.cos() * SHIP_RADIUS,
            self.b.y - self.angle.sin() * SHIP_RADIUS,
        )
    }
}

/// A piece of salvage.
#[derive(Clone, Debug)]
pub struct Item {
    /// Stable identity: the tow chain refers to items by id.
    pub id: u32,
    pub kind: ItemType,
    pub b: Body,
    pub label: &'static str,
    pub grade: usize,
    pub tint: Option<u32>,
    pub rare: bool,
    pub hidden: bool,
    pub value: f64,
    /// Hit points; None is indestructible.
    pub hp: Option<f64>,
    pub angle: f64,
    pub spin: f64,
    pub towed: bool,
    /// Rope length to this item's anchor while towed.
    pub link_len: f64,
    /// Velocity change the rope applied this step.
    pub jerk: f64,
    /// Smoothed 0..1 cable tension, for the cable's colour.
    pub tension: f64,
    /// Seconds since being thrown (dropped or cut), counting down.
    pub thrown_t: f64,
    pub fuse: f64,
    pub shape: Vec<ShapePt>,
    pub dead: bool,
}

impl Item {
    pub fn def(&self) -> &'static ItemDef {
        self.kind.def()
    }
}

/// Pick a grade index by weight.
pub fn roll_grade(rng: &mut Rng, def: &ItemDef) -> usize {
    let total: f64 = def.grades.iter().map(|g| g.weight).sum();
    let mut roll = rng.next() * total;
    for (i, g) in def.grades.iter().enumerate() {
        roll -= g.weight;
        if roll < 0.0 {
            return i;
        }
    }
    def.grades.len() - 1
}

/// Create a salvage item, rolling its grade. A ranged value is skewed
/// toward the low end (power 1.6) so a big pod is an occasional jackpot.
/// `belt_value` is the belt's price multiplier (1 in the starting belt).
pub fn make_item(
    rng: &mut Rng,
    id: u32,
    kind: ItemType,
    x: f64,
    y: f64,
    vx: f64,
    vy: f64,
    belt_value: f64,
) -> Item {
    let def = kind.def();
    let gi = roll_grade(rng, def);
    let g = &def.grades[gi];
    let base = match g.value {
        Value::Fixed(v) => v,
        Value::Range(lo, hi) => lo + (hi - lo) * rng.next().powf(1.6),
    };
    let value = if base != 0.0 {
        js::round(base * belt_value / 10.0) * 10.0
    } else {
        0.0
    };
    let angle = rng.angle();
    let spin = rng.range(-1.2, 1.2);
    let shape = jagged_shape(rng, 7, 0.6, 1.15);
    Item {
        id,
        kind,
        b: Body {
            x,
            y,
            vx,
            vy,
            r: def.r,
            mass: js::round(def.mass * g.mass_mul * 10.0) / 10.0,
            fixed: false,
        },
        label: g.name,
        grade: gi,
        tint: g.tint,
        rare: def.grades.len() > 1 && gi == def.grades.len() - 1,
        hidden: def.hidden,
        value,
        hp: g.hp.or(def.hp),
        angle,
        spin,
        towed: false,
        link_len: 0.0,
        jerk: 0.0,
        tension: 0.0,
        thrown_t: 0.0,
        fuse: 0.0,
        shape,
        dead: false,
    }
}

/// A crater on a rock, as fractions of its radius.
#[derive(Clone, Copy, Debug)]
pub struct Crater {
    pub a: f64,
    pub d: f64,
    pub s: f64,
}

#[derive(Clone, Debug)]
pub struct Rock {
    pub size: u8,
    pub b: Body,
    pub hp: f64,
    /// Gold-seamed rock drops ore when broken.
    pub veined: bool,
    pub angle: f64,
    pub spin: f64,
    pub shape: Vec<ShapePt>,
    pub craters: Vec<Crater>,
    pub flash: f64,
    /// Pebbles from breakage crumble after this many seconds.
    pub crumble_t: Option<f64>,
    pub dead: bool,
}

/// `makeRock(size, x, y, vx, vy, veined)`. Mass comes from the size's
/// nominal radius, not the rolled one, as in the browser.
#[allow(clippy::too_many_arguments)]
pub fn make_rock(rng: &mut Rng, size: u8, x: f64, y: f64, vx: f64, vy: f64, veined: bool) -> Rock {
    let (def_r, hp) = rock_size(size);
    let r = def_r * rng.range(0.9, 1.1);
    let angle = rng.angle();
    let spin = rng.range(-0.5, 0.5);
    let n = rng.int(9, 13);
    let shape = jagged_shape(rng, n, 0.72, 1.12);
    let crater_count = if size == 1 { 1 } else { rng.int(2, 4) };
    let craters = (0..crater_count)
        .map(|_| Crater {
            a: rng.angle(),
            d: rng.range(0.1, 0.55),
            s: rng.range(0.1, 0.2),
        })
        .collect();
    Rock {
        size,
        b: Body {
            x,
            y,
            vx,
            vy,
            r,
            mass: def_r * def_r / 180.0,
            fixed: false,
        },
        hp,
        veined,
        angle,
        spin,
        shape,
        craters,
        flash: 0.0,
        crumble_t: None,
        dead: false,
    }
}

/// A rock with the browser's default drift and vein rolls.
pub fn make_rock_drifting(rng: &mut Rng, size: u8, x: f64, y: f64) -> Rock {
    let vx = rng.range(-25.0, 25.0);
    let vy = rng.range(-25.0, 25.0);
    let veined = rng.chance(0.12);
    make_rock(rng, size, x, y, vx, vy, veined)
}

/// A derelict hull. In this milestone wrecks drift and collide; cutting
/// them open (hp, contents, crabs) arrives with combat in milestone 2.
#[derive(Clone, Debug)]
pub struct Wreck {
    pub b: Body,
    pub angle: f64,
    pub spin: f64,
    pub hp: f64,
    pub max_hp: f64,
    /// Item types ejected as the hull is cut apart, in order.
    pub contents: Vec<ItemType>,
    pub crabs: i32,
    /// Hull outline in local coordinates.
    pub pts: Vec<(f64, f64)>,
    pub flash: f64,
    /// Phase of the blinking emergency light.
    pub light: f64,
    pub dead: bool,
}

/// `makeWreck(x, y)`. `shift_no` sets how many crabs hide inside.
pub fn make_wreck(rng: &mut Rng, x: f64, y: f64, shift_no: i32) -> Wreck {
    use ItemType::*;
    let r = rng.range(85.0, 120.0);
    let mut contents = vec![Plate, Plate, Scrap];
    if rng.chance(0.5) {
        contents.push(Scrap);
    }
    if rng.chance(0.6) {
        contents.push(Engine);
    }
    if rng.chance(0.4) {
        contents.push(Reactor);
    }
    if rng.chance(0.15) {
        contents.push(Blackbox);
    }
    if rng.chance(0.2) {
        contents.push(Pod);
    }
    // Shuffle so the most valuable piece is not always last out.
    for i in (1..contents.len()).rev() {
        let j = rng.int(0, i as i32) as usize;
        contents.swap(i, j);
    }
    // Hull silhouette: an elongated, notched ellipse.
    let n = 16;
    let pts = (0..n)
        .map(|i| {
            let a = (f64::from(i) / f64::from(n)) * TAU;
            let notch = if rng.chance(0.25) {
                rng.range(0.55, 0.8)
            } else {
                rng.range(0.92, 1.05)
            };
            (a.cos() * r * 1.35 * notch, a.sin() * r * 0.62 * notch)
        })
        .collect();
    let hp = 24.0 + contents.len() as f64 * 3.0;
    let vx = rng.range(-8.0, 8.0);
    let vy = rng.range(-8.0, 8.0);
    let angle = rng.angle();
    let spin = rng.range(-0.06, 0.06);
    let crabs = (rng.int(1, 2) + (shift_no - 1) / 2).min(4);
    Wreck {
        b: Body {
            x,
            y,
            vx,
            vy,
            r,
            mass: 60.0,
            fixed: false,
        },
        angle,
        spin,
        hp,
        max_hp: hp,
        contents,
        crabs,
        pts,
        flash: 0.0,
        light: rng.angle(),
        dead: false,
    }
}
