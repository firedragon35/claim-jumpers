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

    /// Distance between this body's centre and `o`'s centre, in px.
    pub fn dist_to(&self, o: &Body) -> f64 {
        js::dist(self.x, self.y, o.x, o.y)
    }

    /// Speed (the length of the velocity vector), in px/s.
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
    /// Seconds until the cannon can fire again.
    pub fire_t: f64,
    /// Gun heat 0..100; at 100 the cannon locks until it cools.
    pub heat: f64,
    pub overheated: bool,
    /// Seconds until the hull-alarm or under-attack loops sound again.
    pub alarm_t: f64,
    /// Grinder slugs loaded; they fire instead of cutter rounds.
    pub slugs: u32,
    /// Side-thruster input this step, -1..1 (0 without the part).
    pub strafe: f64,
    /// Caught in a debt collector's net.
    pub net: Option<crate::opposition::NetHold>,
    /// The last hard turn direction, for counting reversals that tear a net.
    pub last_turn_sign: f64,
    /// Seconds left slowed by a sheriff's stun bolt.
    pub stun_t: f64,
    /// Afterburner: seconds of burst left, engine heat 0..100, seconds of
    /// overheat penalty left, and whether the button was held last step.
    pub burn_t: f64,
    pub engine_heat: f64,
    pub engine_over_t: f64,
    pub prev_burn: bool,
    /// Missiles in the rack, and the reload and launch-gap timers.
    pub missiles: u32,
    pub missile_reload_t: f64,
    pub missile_gap_t: f64,
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
            fire_t: 0.0,
            heat: 0.0,
            overheated: false,
            alarm_t: 0.0,
            slugs: 0,
            strafe: 0.0,
            net: None,
            last_turn_sign: 0.0,
            stun_t: 0.0,
            burn_t: 0.0,
            engine_heat: 0.0,
            engine_over_t: 0.0,
            prev_burn: false,
            missiles: mods.missile_rack,
            missile_reload_t: 0.0,
            missile_gap_t: 0.0,
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
    /// Seconds until a primed reactor blows; 0 when not primed.
    pub fuse: f64,
    /// The rival (jumper id) dragging this item off, if any.
    pub hooked_by: Option<u32>,
    /// Seconds until a towed rogue core next tries to zap the tug.
    pub zap_t: f64,
    pub shape: Vec<ShapePt>,
    pub dead: bool,
}

impl Item {
    /// The static definition (grades and base properties) of this item's type.
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
/// `belt_value` is the belt's price multiplier (1 in the starting belt);
/// `cond_value` is the belt condition's multiplier for this item type.
// Mirrors the browser's makeItem, which reads the belt and condition from
// global state; here they are passed in, so the argument list is long.
#[allow(clippy::too_many_arguments)]
pub fn make_item(
    rng: &mut Rng,
    id: u32,
    kind: ItemType,
    x: f64,
    y: f64,
    vx: f64,
    vy: f64,
    belt_value: f64,
    cond_value: f64,
) -> Item {
    let def = kind.def();
    let gi = roll_grade(rng, def);
    let g = &def.grades[gi];
    let base = match g.value {
        Value::Fixed(v) => v,
        Value::Range(lo, hi) => lo + (hi - lo) * rng.next().powf(1.6),
    };
    let value = if base != 0.0 {
        // Richer belts and belt conditions move prices.
        js::round(base * belt_value * cond_value / 10.0) * 10.0
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
        hooked_by: None,
        zap_t: 1.0,
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

/// A breakable asteroid. Each size has its own radius and hit points, and a veined rock drops ore when broken.
#[derive(Clone, Debug)]
pub struct Rock {
    /// Stable identity (a miner keeps cutting the same rock).
    pub id: u32,
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
        id: next_rock_id(),
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

/// A fresh rock identity. Rocks are made by free functions without the
/// world at hand, so they draw from a process-wide counter; ids are only
/// ever compared for identity, never fed into any numbers.
pub fn next_rock_id() -> u32 {
    use std::sync::atomic::{AtomicU32, Ordering};
    static NEXT: AtomicU32 = AtomicU32::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// A rock with the browser's default drift and vein rolls.
pub fn make_rock_drifting(rng: &mut Rng, size: u8, x: f64, y: f64) -> Rock {
    let vx = rng.range(-25.0, 25.0);
    let vy = rng.range(-25.0, 25.0);
    let veined = rng.chance(0.12);
    make_rock(rng, size, x, y, vx, vy, veined)
}

/// A derelict hull: it drifts, collides, and can be cut open for its contents.
#[derive(Clone, Debug)]
pub struct Wreck {
    pub b: Body,
    /// A sealed hold: only its keycard opens it.
    pub locked: bool,
    /// The derelict freighter of a field event.
    pub freighter: bool,
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
        locked: false,
        freighter: false,
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

/// What a rival claim jumper is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JumperState {
    /// Looking for cargo to steal (or circling the tug if there is none).
    Hunt,
    /// Latched onto a piece and dragging it out of the field.
    Haul,
}

/// A rival claim jumper.
#[derive(Clone, Debug)]
pub struct Jumper {
    pub id: u32,
    pub variant: Variant,
    /// A named boss crew and the bounty on it.
    pub boss: Option<Boss>,
    pub b: Body,
    pub hp: f64,
    pub angle: f64,
    pub state: JumperState,
    /// Item id it is heading for.
    pub target: Option<u32>,
    /// Item id it has hooked.
    pub hook: Option<u32>,
    pub link_len: f64,
    /// Seconds latched onto your train (it cuts the line at the sever time).
    pub latch_t: f64,
    pub fire_t: f64,
    pub retarget: f64,
    pub flash: f64,
    /// Phase angle while circling the tug.
    pub orbit: f64,
    /// Hunters skip the cargo and go straight for the tug.
    pub hunter: bool,
    pub speed_mul: f64,
    /// Seconds left fleeing after a wingmate was smashed.
    pub flee_t: f64,
    /// Seconds left giving covering fire to a latched wingmate.
    pub cover_t: f64,
    pub dead: bool,
}

/// Pick a hull variant, weighted by the rivals' grudge stage.
pub fn pick_variant(rng: &mut Rng, stage: usize) -> Variant {
    let total: f64 = VARIANTS.iter().map(|v| v.def().weights[stage]).sum();
    let mut roll = rng.next() * total;
    for v in VARIANTS {
        roll -= v.def().weights[stage];
        if roll < 0.0 {
            return v;
        }
    }
    Variant::Cutter
}

/// `makeJumper(x, y, variant)`: a rival toughened by the grudge stage.
/// `variant` None rolls one for the stage.
pub fn make_jumper(
    rng: &mut Rng,
    id: u32,
    x: f64,
    y: f64,
    variant: Option<Variant>,
    stage: usize,
    hunter_mul: f64,
) -> Jumper {
    let st = &RIVAL_STAGES[stage];
    let variant = variant.unwrap_or_else(|| pick_variant(rng, stage));
    let v = variant.def();
    let fire_t = rng.range(1.0, 2.5);
    let orbit = rng.angle();
    // Crews with good standing send half as many hunters (`hunter_mul`).
    let hunter = v.hunter || rng.next() < st.hunters * hunter_mul;
    Jumper {
        id,
        variant,
        boss: None,
        b: Body {
            x,
            y,
            r: v.r,
            mass: v.mass,
            ..Body::default()
        },
        hp: (JUMPER_HP + st.hp + v.hp).max(2.0),
        angle: 0.0,
        state: JumperState::Hunt,
        target: None,
        hook: None,
        link_len: 0.0,
        latch_t: 0.0,
        fire_t,
        retarget: 0.0,
        flash: 0.0,
        orbit,
        hunter,
        speed_mul: st.speed * v.speed,
        flee_t: 0.0,
        cover_t: 0.0,
        dead: false,
    }
}

/// What a scrap crab is doing (see updateCrab).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrabState {
    /// Sitting on a salvage piece; starts crawling once it is towed.
    Ride,
    /// Walking a cable link toward the tug, hopping link to link.
    Crawl,
    /// On the tug's hull, chewing it until spun off or repaired at the bay.
    Latched,
    /// Drifting; homes in on the tug or nearby cargo.
    Free,
}

/// A scrap crab: it rides salvage, crawls along the cable toward the tug and then chews the hull (see `CrabState`).
#[derive(Clone, Debug)]
pub struct Crab {
    pub state: CrabState,
    /// Item id it rides or crawls from.
    pub host: Option<u32>,
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
    pub r: f64,
    /// Progress 0..1 along the current cable link.
    pub t: f64,
    pub ride_t: f64,
    pub stun_t: f64,
    /// Angle offset of its perch on the host or hull.
    pub off_a: f64,
    /// Leg animation phase.
    pub leg_t: f64,
    pub chomp_t: f64,
    pub dead: bool,
}

/// Creates a crab in `state` at (`x`, `y`), riding or crawling from the item `host`, with a random perch angle and leg phase.
pub fn make_crab(rng: &mut Rng, state: CrabState, host: Option<u32>, x: f64, y: f64) -> Crab {
    let off_a = rng.angle();
    let leg_t = rng.angle();
    Crab {
        state,
        host,
        x,
        y,
        vx: 0.0,
        vy: 0.0,
        r: CRAB_RADIUS,
        t: 0.0,
        ride_t: 0.0,
        stun_t: 0.0,
        off_a,
        leg_t,
        chomp_t: 0.0,
        dead: false,
    }
}

/// A cannon round (yours) or a bolt (theirs).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Bullet {
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
    pub life: f64,
    pub dmg: f64,
    /// A heavy grinder slug rather than a cutter round.
    pub slug: bool,
    /// Fired by one of your sentries (passes through the law).
    pub sentry: bool,
    /// A sheriff's bolt: stuns (or SWAT's real round).
    pub stun: bool,
    pub swat: bool,
    /// A rogue sentry's bolt, and which sentry fired it.
    pub rogue: bool,
    pub owner: Option<u32>,
}

/// A named rival boss crew (a field event).
#[derive(Clone, Debug, PartialEq)]
pub struct Boss {
    pub name: &'static str,
    pub bounty: f64,
}
