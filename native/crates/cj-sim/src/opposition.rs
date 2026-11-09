//! The opposition (src/js/16-simulation-step.js): the Kessler refinery and
//! its security drones and jaws, debt collectors and their nets, the law
//! (sheriffs, marshals and SWAT), auto-defense missiles and torpedoes,
//! sentry drones (and the rogues they become), and the mining crew.
//!
//! Every hostile has a stable id from the world's counter, so missiles,
//! nets and the tug's net can refer to one that may since have been swept.

use crate::career::{tier, Tier};
use crate::consts::*;
use crate::entities::{Body, Bullet, CrabState};
use crate::events::{Event, Radio, Sfx, Tint};
use crate::items::ItemType;
use crate::js;
use crate::rivals::steer;
use crate::world::{fmt_credits, World};
use std::f64::consts::{PI, TAU};

// ---------------------------------------------------------------------
// Tuning (01-tuning-constants.js and the opposition's own constants).
// ---------------------------------------------------------------------

/// Afterburner: seconds per burst, thrust multiplier, engine heat per
/// burst (100 overheats), cooling per second, and the overheat penalty.
pub const BURN_TIME: f64 = 3.0;
/// Thrust multiplier while an afterburner burst lasts.
pub const BURN_THRUST: f64 = 2.3;
/// Engine heat added by one burst; the engine overheats at 100.
pub const BURN_HEAT: f64 = 34.0;
/// Engine heat shed per second while no burst is running.
pub const ENGINE_COOL: f64 = 7.0;
/// Seconds the engine stays crippled after overheating.
pub const ENGINE_OVERHEAT_TIME: f64 = 6.0;
/// Thrust multiplier while the engine is overheated.
pub const ENGINE_OVERHEAT_THRUST: f64 = 0.45;

/// Seconds between volleys from the auto-defense rack.
pub const MISSILE_GAP: f64 = 0.5;
/// How far a missile launch looks for a hostile, in px.
pub const MISSILE_RANGE: f64 = 650.0;
/// Range in px at which incoming nets are targeted before any other hostile.
pub const MISSILE_NET_RANGE: f64 = 420.0;
/// Speed missiles steer toward, in px/s.
pub const MISSILE_SPEED: f64 = 620.0;
/// How sharply a missile steers: velocity closes this fraction per second on its target velocity.
pub const MISSILE_TURN: f64 = 6.0;
/// Seconds a missile flies before it fizzles; torpedoes last 1.5 times as long.
pub const MISSILE_LIFE: f64 = 3.0;
/// Torpedo speed and turn rate as a fraction of a missile's.
pub const TORPEDO_SPEED: f64 = 0.7;
/// Torpedo blast radius, in px; damage falls off linearly to zero at the edge.
pub const TORPEDO_SPLASH: f64 = 120.0;
/// Torpedo splash damage at the blast centre.
pub const TORPEDO_SPLASH_DAMAGE: f64 = 4.0;

/// Radius of a sentry drone's orbit around the tug, in px.
pub const SENTRY_ORBIT: f64 = 75.0;
/// How far a sentry drone looks for a hostile, in px.
pub const SENTRY_RANGE: f64 = 450.0;
/// Damage of one sentry drone shot (a rogue's bolt hitting a drone does twice this).
pub const SENTRY_DAMAGE: f64 = 0.6;
/// How far a rogue drone looks for a target, in px.
pub const ROGUE_RANGE: f64 = 480.0;
/// Seconds between a rogue drone's shots.
pub const ROGUE_FIRE: f64 = 1.1;
/// Damage of one rogue drone bolt.
pub const ROGUE_DAMAGE: f64 = 3.0;
/// Speed of a rogue drone, in px/s.
pub const ROGUE_SPEED: f64 = 150.0;

/// Seconds before the refinery arrives on shifts after the first (shift 1 waits 75).
pub const REFINERY_ARRIVE_AT: f64 = 40.0;
/// Radius of the refinery's hub, in px.
pub const REFINERY_HUB_R: f64 = 150.0;
/// Distance of each vault socket from the refinery's centre, in px.
pub const SOCKET_RADIUS: f64 = 178.0;
/// How close a cargo item must be to an empty socket to fill it, in px.
pub const SOCKET_CAPTURE: f64 = 80.0;
/// Distance of the maw behind the refinery's centre, in px.
pub const MAW_OFFSET: f64 = 250.0;
/// Radius of the maw, in px: the area that eats cargo and takes hits.
pub const MAW_R: f64 = 85.0;
/// Strength of the maw's pull, an acceleration scaled by its hunger.
pub const JAW_PULL: f64 = 170.0;
/// Starting distance the maw pulls from, in px.
pub const JAW_RANGE_START: f64 = 550.0;
/// Greatest distance the maw can pull from, in px; it grows as it eats.
pub const JAW_RANGE_MAX: f64 = 850.0;
/// Hit points of the jaws.
pub const JAW_HP: f64 = 60.0;
/// Damage to the jaws from swallowing something volatile.
pub const JAW_FEED_DAMAGE: f64 = 20.0;
/// Seconds of countdown once all three sockets are filled.
pub const ARM_SECONDS: f64 = 5.0;
/// Vault loot: six ingots and eight ore, in that order.
pub const VAULT_LOOT: [(ItemType, u32); 2] = [(ItemType::Ingot, 6), (ItemType::Ore, 8)];

/// Hit points of a debt collector.
pub const COLLECTOR_HP: f64 = 8.0;
/// Mass of a debt collector's body.
pub const COLLECTOR_MASS: f64 = 6.0;
/// Acceleration, in px/s², with which a collector hauls a netted tug toward its exit.
pub const COLLECTOR_HAUL_ACCEL: f64 = 260.0;
/// Cutter rounds do a third; slugs, missiles and cargo do full damage.
pub const COLLECTOR_BULLET_ARMOR: f64 = 0.35;
/// Most collectors on the field at once.
pub const COLLECTOR_MAX: usize = 2;
/// Distance a collector keeps from the tug when payment is at risk or it is greedy, in px.
pub const COLLECTOR_HOLD: f64 = 380.0;
/// How far off a collector watches when your payment is covered, and for how long.
pub const COLLECTOR_SHADOW: f64 = 520.0;
/// Seconds a collector shadows a tug with payment covered before it leaves.
pub const COLLECTOR_SHADOW_TIME: f64 = 30.0;
/// Per second while shadowing a tug with cargo: jack the haul.
pub const COLLECTOR_GREED_CHANCE: f64 = 1.0 / 250.0;
/// Range at which a collector fires its net, in px.
pub const COLLECTOR_FIRE_RANGE: f64 = 560.0;
/// Collectors won't fire this close to the carrier's bay.
pub const BAY_SANCTUARY: f64 = 450.0;
/// Speed of a collector's net, in px/s.
pub const NET_SPEED: f64 = 420.0;
/// Seconds a net flies before it vanishes.
pub const NET_LIFE: f64 = 1.8;
/// Radius of a net, in px, for hitting the tug.
pub const NET_RADIUS: f64 = 20.0;
/// Hit points of a net on the tug; tearing it free depletes them.
pub const NET_HP: f64 = 100.0;
/// Net damage per thrust reversal (scaled by the net-tear factor).
pub const NET_TEAR_PER_REVERSAL: f64 = 7.0;
/// Net damage per second at full strafe.
pub const NET_TEAR_STRAFE: f64 = 25.0;
/// Net damage per second while thrusting.
pub const NET_TEAR_THRUST: f64 = 4.0;
/// Thrust multiplier while the tug is netted.
pub const NETTED_THRUST: f64 = 0.35;
/// Base impound fee, before the difficulty and crew multipliers.
pub const IMPOUND_FEE: f64 = 1500.0;
/// Seconds into the shift at which an unpaid installment summons a collector.
pub const LOW_WALLET_CHECK_AT: f64 = 150.0;
/// Cable value at which a collector confiscates the load instead of impounding the tug.
pub const FAT_CABLE_VALUE: f64 = 2000.0;
/// Seconds before a fat cable can summon a collector again.
pub const FAT_CABLE_COOLDOWN: f64 = 60.0;
/// Fraction of a confiscated load's value credited as payment.
pub const CONFISCATE_RATE: f64 = 0.6;
/// Range of seconds before a collector that took its payment is replaced.
pub const COLLECTOR_RETURN: (f64, f64) = (60.0, 90.0);

/// Most wanted stars.
pub const WANTED_MAX: u32 = 5;
/// Range at which a sheriff spots a one-star tug, in px.
pub const SHERIFF_DETECT: f64 = 650.0;
/// Extra spotting range per additional star, in px.
pub const SHERIFF_DETECT_PER_STAR: f64 = 150.0;
/// A sheriff starts losing the tug beyond its spotting range times this.
pub const SHERIFF_LOSE_MULT: f64 = 1.7;
/// Seconds out of range before a sheriff gives up the chase.
pub const SHERIFF_LOSE_TIME: f64 = 7.0;
/// Distance within which a sheriff's cuff meter fills, in px.
pub const SHERIFF_CUFF_RANGE: f64 = 150.0;
/// A new sheriff's starting patrol timer, in seconds.
pub const SHERIFF_PATROL_TIME: f64 = 75.0;
/// Fine per wanted star on arrest, in credits.
pub const FINE_PER_STAR: f64 = 800.0;
/// Seconds a sheriff's stun bolt leaves the tug sluggish.
pub const STUN_TIME: f64 = 1.5;
/// Per second, a patrol near a clean tug with cargo shakes it down.
pub const SHAKEDOWN_CHANCE: f64 = 1.0 / 100.0;

/// Hit points of a mining-crew tug.
pub const MINER_HP: f64 = 5.0;
/// How close a miner must be to a rock's edge to drill it, in px.
pub const MINER_BEAM_RANGE: f64 = 150.0;
/// Distance from a rock's edge at which a miner parks, in px.
pub const MINER_STANDOFF: f64 = 90.0;
/// Drill progress per second; each full point hits the rock for 1 damage.
pub const MINER_DRILL_RATE: f64 = 1.4;
/// Extra speed, in px/s, the pieces of a rock a miner breaks are flung with.
pub const MINER_KICK: f64 = 140.0;
/// Chance a rock of size 2 or more a miner breaks spills a piece of ore.
pub const MINER_ORE_CHANCE: f64 = 0.25;
/// Seconds a miner stays angry after being shot.
pub const MINER_ANGRY_TIME: f64 = 5.0;
/// Damage per second of an angry miner's beam on the tug.
pub const MINER_BEAM_DPS: f64 = 10.0;
/// Range at which an angry miner's beam hits the tug, in px.
pub const MINER_ANGRY_RANGE: f64 = 260.0;

/// Spotting range for a wanted tug.
pub fn detect_range(stars: u32) -> f64 {
    SHERIFF_DETECT + SHERIFF_DETECT_PER_STAR * f64::from(stars.max(1) - 1)
}

/// Mining crew size: 2 on shift 1, 3 on shifts 2-3, then 4.
pub fn miner_crew_for(shift: i32) -> i32 {
    if shift == 1 {
        2
    } else if shift <= 3 {
        3
    } else {
        4
    }
}

/// The law's response per wanted level (LAW_TIERS, 1..=5).
pub struct LawTier {
    pub name: &'static str,
    pub units: usize,
    pub hp: f64,
    pub speed: f64,
    pub fire: (f64, f64),
    pub dmg: f64,
    pub stun: bool,
    pub cuff: f64,
    pub swat: bool,
}

/// The law's response for each wanted level; index 0 is one star.
#[rustfmt::skip]
pub const LAW_TIERS: [LawTier; 5] = [
    LawTier { name: "Sheriff", units: 1, hp: 14.0, speed: 430.0, fire: (1.4, 2.0), dmg: 3.0, stun: true, cuff: 3.0, swat: false },
    LawTier { name: "Sheriff", units: 1, hp: 16.0, speed: 470.0, fire: (1.1, 1.6), dmg: 3.0, stun: true, cuff: 3.0, swat: false },
    LawTier { name: "Sheriff + deputy", units: 2, hp: 16.0, speed: 480.0, fire: (1.0, 1.5), dmg: 4.0, stun: true, cuff: 2.6, swat: false },
    LawTier { name: "Marshals", units: 3, hp: 20.0, speed: 500.0, fire: (0.9, 1.3), dmg: 5.0, stun: true, cuff: 2.3, swat: false },
    LawTier { name: "Galactic SWAT", units: 4, hp: 26.0, speed: 540.0, fire: (0.6, 0.9), dmg: 7.0, stun: false, cuff: 2.0, swat: true },
];

/// The law's response for a star count (capped at `WANTED_MAX`); `None` when not wanted.
pub fn law_tier(stars: u32) -> Option<&'static LawTier> {
    (stars >= 1).then(|| &LAW_TIERS[(stars.min(WANTED_MAX) - 1) as usize])
}

/// Standing changes per event (REP), as (bank, law, rivals).
const REP_STAR_GAINED_LAW: f64 = -6.0;
const REP_COLLECTOR_KILL_BANK: f64 = -12.0;
const REP_LAW_KILL_LAW: f64 = -10.0;
const REP_ARRESTED_LAW: f64 = -4.0;
const REP_IMPOUNDED_BANK: f64 = -5.0;

// ---------------------------------------------------------------------
// Entities.
// ---------------------------------------------------------------------

/// A refinery security drone.
#[derive(Clone, Debug)]
pub struct Drone {
    pub id: u32,
    pub b: Body,
    pub hp: f64,
    pub angle: f64,
    pub orbit: f64,
    pub fire_t: f64,
    pub flash: f64,
    pub dead: bool,
}

/// What a debt collector is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollectorState {
    /// Hold station off the tug and fire nets when lined up (or shadow it).
    Approach,
    /// Net landed: haul the tug toward the nearest field edge.
    Tow,
    /// Net torn or dropped: back off, then try again.
    Recover,
    /// Payment looks covered: head out of the field.
    Leave,
}

/// A debt collector: shadows the tug, then nets and tows it when payment is at risk.
#[derive(Clone, Debug)]
pub struct Collector {
    pub id: u32,
    pub b: Body,
    pub hp: f64,
    pub angle: f64,
    pub state: CollectorState,
    pub fire_t: f64,
    pub tether: f64,
    /// Unit vector toward the field edge it tows to.
    pub exit: (f64, f64),
    pub recover_t: f64,
    pub flash: f64,
    pub edge_warned: bool,
    pub greedy: bool,
    pub greed_roll_t: f64,
    pub calm_t: f64,
    /// You hit it yourself: your auto-defenses now engage it.
    pub provoked: bool,
    /// Eaten by the jaws (not your doing).
    pub by_jaws: bool,
    pub dead: bool,
}

/// A debt collector's net in flight.
#[derive(Clone, Debug)]
pub struct Net {
    pub id: u32,
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
    pub life: f64,
    pub spin: f64,
    /// The collector that fired it.
    pub owner: u32,
}

/// The tug caught in a net.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NetHold {
    /// The collector holding the tether.
    pub by: u32,
    pub hp: f64,
}

/// What a sheriff (or other law unit) is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SheriffState {
    Patrol,
    Pursuit,
    Leave,
}

/// A law unit: sheriff, deputy, marshal or SWAT, depending on the wanted level.
#[derive(Clone, Debug)]
pub struct Sheriff {
    pub id: u32,
    pub b: Body,
    pub hp: f64,
    pub angle: f64,
    pub state: SheriffState,
    pub wp: Option<(f64, f64)>,
    pub patrol_t: f64,
    pub fire_t: f64,
    pub cuff: f64,
    pub lost_t: f64,
    pub siren_t: f64,
    pub flash: f64,
    pub speed: f64,
    pub fire: (f64, f64),
    pub dmg: f64,
    pub stun: bool,
    pub cuff_time: f64,
    pub swat: bool,
    pub shakedown: bool,
    pub shake_roll_t: f64,
    pub warned: bool,
    pub provoked: bool,
    pub by_jaws: bool,
    pub dead: bool,
}

/// A mining-crew tug that drills rocks and turns on the player if shot.
#[derive(Clone, Debug)]
pub struct Miner {
    pub id: u32,
    pub b: Body,
    pub hp: f64,
    pub angle: f64,
    /// The rock (by id) it is cutting.
    pub target: Option<u32>,
    /// Where the cutting beam lands this step, if it is cutting.
    pub beam: Option<(f64, f64)>,
    pub drill: f64,
    pub angry_t: f64,
    pub retarget: f64,
    pub zap_t: f64,
    pub flash: f64,
    pub dead: bool,
}

/// What a missile is chasing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tgt {
    Net(u32),
    Jumper(u32),
    Drone(u32),
    Collector(u32),
    Sheriff(u32),
    Miner(u32),
    Sentry(u32),
    Jaws,
}

/// An auto-defense missile or torpedo in flight.
#[derive(Clone, Debug)]
pub struct Missile {
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
    pub angle: f64,
    pub target: Option<Tgt>,
    pub life: f64,
    pub torpedo: bool,
}

/// A sentry drone orbiting the tug; every survivor goes rogue when the tug is gone.
#[derive(Clone, Debug)]
pub struct Sentry {
    pub id: u32,
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
    pub r: f64,
    pub hp: f64,
    pub orbit: f64,
    pub fire_t: f64,
    pub angle: f64,
    pub flash: f64,
    pub rogue: bool,
    pub wander: (f64, f64),
    pub dead: bool,
}

/// The stages of the Kessler refinery's vault and jaws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefineryState {
    /// Arrived, quiet; charges float nearby.
    Dormant,
    /// Security drones awake.
    Locked,
    /// All three sockets hold charges; counting down.
    Armed,
    /// Vault blown, loot out; a short grace period.
    Open,
    /// Processing jaws awake: everything nearby is pulled in and eaten.
    Hungry,
    /// Jaws destroyed.
    Wrecked,
}

/// One of the refinery's three vault sockets: its angle on the station and whether it holds a charge.
#[derive(Clone, Copy, Debug)]
pub struct Socket {
    pub a: f64,
    pub filled: bool,
    pub spent: bool,
}

/// The Kessler refinery: a rotating station with a vault, security drones and processing jaws.
#[derive(Clone, Debug)]
pub struct Refinery {
    pub b: Body,
    pub angle: f64,
    pub spin: f64,
    pub state: RefineryState,
    pub arm_t: f64,
    pub wake_t: f64,
    pub drone_t: f64,
    pub hunger: f64,
    pub chomp: f64,
    pub eaten: u32,
    pub pull_r: f64,
    pub hint_t: f64,
    pub last_beep: i64,
    pub jaw_hp: f64,
    pub jaw_flash: f64,
    pub sockets: [Socket; 3],
}

impl Refinery {
    /// World position of a socket on the rotating station.
    pub fn socket_pos(&self, k: usize) -> (f64, f64) {
        let a = self.angle + self.sockets[k].a;
        (
            self.b.x + a.cos() * SOCKET_RADIUS,
            self.b.y + a.sin() * SOCKET_RADIUS,
        )
    }
    /// The maw between the jaws, on the station's local -x side.
    pub fn maw(&self) -> (f64, f64) {
        (
            self.b.x - self.angle.cos() * MAW_OFFSET,
            self.b.y - self.angle.sin() * MAW_OFFSET,
        )
    }
    /// The jaws can be damaged once awake (vault open or feeding).
    pub fn jaws_vulnerable(&self) -> bool {
        matches!(self.state, RefineryState::Open | RefineryState::Hungry)
    }
    /// The jaw target missiles and blasts aim at: the maw.
    pub fn jaw_body(&self) -> Body {
        let (x, y) = self.maw();
        Body {
            x,
            y,
            vx: self.b.vx,
            vy: self.b.vy,
            r: MAW_R * 0.7,
            mass: 1.0,
            fixed: false,
        }
    }
}

/// Unit vector toward the field edge nearest to (x, y) (nearestExit).
pub fn nearest_exit(x: f64, y: f64) -> (f64, f64) {
    let opts = [
        (x, (-1.0, 0.0)),
        (WORLD_W - x, (1.0, 0.0)),
        (y, (0.0, -1.0)),
        (WORLD_H - y, (0.0, 1.0)),
    ];
    // A stable sort by distance: ties keep this order, as in the browser.
    let mut best = opts[0];
    for o in &opts[1..] {
        if o.0 < best.0 {
            best = *o;
        }
    }
    best.1
}

fn out_of_field(x: f64, y: f64, m: f64) -> bool {
    x < -m || y < -m || x > WORLD_W + m || y > WORLD_H + m
}

impl World {
    /// Hands out the next unique hostile id from the world's counter.
    pub(crate) fn fresh_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// A faction's standing tier for the career being played (neutral
    /// with no career).
    pub(crate) fn standing_bank(&self) -> Tier {
        self.career
            .as_ref()
            .map_or(Tier::Neutral, |c| tier(c.rep.bank))
    }
    /// The law's standing tier for the career being played (neutral with no career).
    pub(crate) fn standing_law(&self) -> Tier {
        self.career
            .as_ref()
            .map_or(Tier::Neutral, |c| tier(c.rep.law))
    }

    fn count_kill(&mut self, f: impl FnOnce(&mut crate::world::Kills)) {
        if !self.no_blame {
            f(&mut self.kills);
        }
    }

    /* ----------------------------- Afterburner ---------------------------- */
    // (Applied inside update_ship; see world.rs.)

    /* ------------------------------ Refinery ------------------------------ */

    /// Places the refinery 1,700 to 2,000 px from the carrier, away from the bay, with a random drift and spin.
    pub fn spawn_refinery(&mut self) {
        // Far enough from the carrier that the jaws can never reach the bay.
        let a = self.rng.range(0.0, TAU);
        let d = self.rng.range(1700.0, 2000.0);
        let x = (CARRIER_X + a.cos() * d).clamp(600.0, WORLD_W - 600.0);
        let y = (CARRIER_Y + a.sin() * d).clamp(600.0, WORLD_H - 600.0);
        let da = self.rng.range(0.0, TAU);
        let drift = 6.0;
        let angle = self.rng.range(0.0, TAU);
        let spin = 0.05 * if self.rng.next() < 0.5 { 1.0 } else { -1.0 };
        let sockets = [-PI / 2.0, 0.0, PI / 2.0].map(|a| Socket {
            a,
            filled: false,
            spent: false,
        });
        let r = Refinery {
            b: Body {
                x,
                y,
                vx: da.cos() * drift,
                vy: da.sin() * drift,
                r: REFINERY_HUB_R,
                mass: f64::INFINITY,
                fixed: true,
            },
            angle,
            spin,
            state: RefineryState::Dormant,
            arm_t: 0.0,
            wake_t: 0.0,
            drone_t: 0.0,
            hunger: 0.0,
            chomp: 0.0,
            eaten: 0,
            pull_r: JAW_RANGE_START,
            hint_t: 6.0,
            last_beep: 0,
            jaw_hp: JAW_HP,
            jaw_flash: 0.0,
            sockets,
        };
        // Clear the station's footprint, jaws included.
        let (mx, my) = r.maw();
        let clear = |o: &Body| {
            o.dist_to(&r.b) < REFINERY_HUB_R + o.r + 140.0
                || js::dist(o.x, o.y, mx, my) < 260.0 + o.r
        };
        for rk in &mut self.rocks {
            if clear(&rk.b) {
                rk.dead = true;
            }
        }
        for w in &mut self.wrecks {
            if clear(&w.b) {
                w.dead = true;
            }
        }
        for i in 0..self.items.len() {
            if !self.items[i].towed && clear(&self.items[i].b) {
                self.remove_item(i);
            }
        }
        // Four charges (one spare) float on the side away from the jaws.
        for _ in 0..4 {
            let ca = r.angle + self.rng.range(-0.5, 0.5);
            let cd = self.rng.range(380.0, 470.0);
            let vx = r.b.vx + self.rng.range(-6.0, 6.0);
            let vy = r.b.vy + self.rng.range(-6.0, 6.0);
            let it = self.new_item(
                ItemType::Charge,
                x + ca.cos() * cd,
                y + ca.sin() * cd,
                vx,
                vy,
            );
            self.items.push(it);
        }
        self.refinery = Some(r);
        self.radio(Radio::Refinery, true, false);
    }

    fn wake_refinery(&mut self) {
        let Some(r) = self.refinery.as_mut() else {
            return;
        };
        if r.state != RefineryState::Dormant {
            return;
        }
        r.state = RefineryState::Locked;
        r.drone_t = 15.0;
        for _ in 0..3 {
            self.spawn_drone();
        }
        self.radio(Radio::Drones, true, false);
    }

    /// Advances the refinery one step: drift and spin, then the logic for its current state.
    pub(crate) fn update_refinery(&mut self, dt: f64) {
        let Some(r) = self.refinery.as_mut() else {
            return;
        };
        r.b.x += r.b.vx * dt;
        r.b.y += r.b.vy * dt;
        r.angle += r.spin * dt;
        r.jaw_flash = (r.jaw_flash - dt).max(0.0);
        if r.state == RefineryState::Wrecked {
            // (Smoke from the broken jaws is drawn by the game.)
            return;
        }
        r.chomp = (r.chomp - dt).max(0.0);
        let mut hint = false;
        if r.hint_t > 0.0 {
            r.hint_t -= dt;
            hint = r.hint_t <= 0.0;
        }
        if hint {
            self.radio(Radio::RefineryHint, true, false);
        }
        let r = self.refinery.as_ref().expect("refinery");
        match r.state {
            RefineryState::Dormant | RefineryState::Locked => {
                let s = &self.ship;
                let near = s.alive && s.b.dist_to(&r.b) < 900.0;
                let towing_charge = self
                    .chain
                    .iter()
                    .filter_map(|id| self.item_index(*id))
                    .any(|i| self.items[i].kind == ItemType::Charge);
                if r.state == RefineryState::Dormant && (near || towing_charge) {
                    self.wake_refinery();
                }
                let r = self.refinery.as_mut().expect("refinery");
                if r.state == RefineryState::Locked {
                    r.drone_t -= dt;
                    if r.drone_t <= 0.0 {
                        r.drone_t = 15.0;
                        if self.drones.len() < 5 {
                            self.spawn_drone();
                        }
                    }
                }
                self.seat_charges();
            }
            RefineryState::Armed => {
                let r = self.refinery.as_mut().expect("refinery");
                r.arm_t -= dt;
                // One beep per remaining second, rising in pitch.
                let sec = r.arm_t.ceil() as i64;
                let beep = sec != r.last_beep;
                if beep {
                    r.last_beep = sec;
                }
                let blow = r.arm_t <= 0.0;
                if beep {
                    self.sfx(Sfx::ArmBeep(sec.clamp(0, 9) as u8));
                }
                if blow {
                    self.blow_vault();
                }
            }
            RefineryState::Open => {
                let r = self.refinery.as_mut().expect("refinery");
                r.wake_t -= dt;
                if r.wake_t <= 0.0 {
                    r.state = RefineryState::Hungry;
                    self.shake = 16.0;
                    self.sfx(Sfx::JawsWake);
                    self.radio(Radio::Jaws, true, false);
                }
            }
            RefineryState::Hungry => {
                let r = self.refinery.as_mut().expect("refinery");
                r.hunger = (r.hunger + dt / 8.0).min(1.0); // the pull ramps up over 8 s
                self.feed_jaws(dt);
            }
            RefineryState::Wrecked => {}
        }
    }

    /// Seat any charge (towed or loose) that reaches an empty socket.
    fn seat_charges(&mut self) {
        let mut i = 0;
        while i < self.items.len() {
            if self.items[i].dead || self.items[i].kind != ItemType::Charge {
                i += 1;
                continue;
            }
            let r = self.refinery.as_ref().expect("refinery");
            let it = &self.items[i];
            let slot = (0..3).find(|&k| {
                let (px, py) = r.socket_pos(k);
                !r.sockets[k].filled && js::dist(it.b.x, it.b.y, px, py) <= SOCKET_CAPTURE
            });
            if let Some(k) = slot {
                let (px, py) = r.socket_pos(k);
                self.wake_refinery();
                self.remove_item(i);
                let r = self.refinery.as_mut().expect("refinery");
                r.sockets[k].filled = true;
                let left = r.sockets.iter().filter(|s| !s.filled).count();
                if left == 0 {
                    r.state = RefineryState::Armed;
                    r.arm_t = ARM_SECONDS;
                    r.last_beep = 0;
                }
                self.sfx(Sfx::Hook);
                self.burst(px, py, 10, Tint::Hazard, 120.0);
                if left == 0 {
                    self.radio(Radio::Armed, true, false);
                } else {
                    let text = format!("Charge seated. {left} to go. Steady hands, kiddo.");
                    self.radio_text(Radio::Seated, true, false, text);
                }
            }
            i += 1;
        }
    }

    /// Detonate the seated charges, open the vault and throw out the loot.
    fn blow_vault(&mut self) {
        let r = self.refinery.as_mut().expect("refinery");
        r.state = RefineryState::Open;
        r.wake_t = 2.5;
        self.vault_opened = true;
        self.emit(Event::Unlock("vault"));
        self.emit(Event::Highlight {
            score: 1800.0,
            text: "Cracked the Kessler vault".to_owned(),
        });
        for k in 0..3 {
            let (px, py) = self.refinery.as_ref().expect("refinery").socket_pos(k);
            self.explode(px, py, 200.0, 40.0);
            let r = self.refinery.as_mut().expect("refinery");
            r.sockets[k].filled = false;
            r.sockets[k].spent = true;
        }
        let r = self.refinery.clone().expect("refinery");
        for (kind, n) in VAULT_LOOT {
            for _ in 0..n {
                // Eject hard and away from the jaws: grab-and-run is
                // possible, but greed still gets you eaten.
                let a = r.angle + self.rng.range(-1.2, 1.2);
                let sp = self.rng.range(160.0, 260.0);
                let off = REFINERY_HUB_R + 25.0;
                let it = self.new_item(
                    kind,
                    r.b.x + a.cos() * off,
                    r.b.y + a.sin() * off,
                    r.b.vx + a.cos() * sp,
                    r.b.vy + a.sin() * sp,
                );
                self.items.push(it);
            }
        }
        self.explosion_fx(r.b.x, r.b.y, 200.0, Tint::Ore);
        self.popup(r.b.x, r.b.y - 40.0, "VAULT OPEN".to_owned(), Tint::Ore);
        self.radio(Radio::Vault, true, false);
    }

    /// Damages the jaws by `n`; ignored unless they are awake (see `jaws_vulnerable`). Wrecks them at zero.
    pub fn hurt_jaws(&mut self, n: f64) {
        let Some(r) = self.refinery.as_mut() else {
            return;
        };
        if !r.jaws_vulnerable() {
            return;
        }
        r.jaw_hp -= n;
        r.jaw_flash = 0.1;
        if r.jaw_hp <= 0.0 {
            self.wreck_jaws();
        }
    }

    /// Jaws destroyed: they stop pulling for good and shed salvage.
    fn wreck_jaws(&mut self) {
        let r = self.refinery.as_mut().expect("refinery");
        r.state = RefineryState::Wrecked;
        r.hunger = 0.0;
        let r = r.clone();
        self.count_kill(|k| k.jaws += 1);
        let (mx, my) = r.maw();
        for kind in [
            ItemType::Engine,
            ItemType::Plate,
            ItemType::Plate,
            ItemType::Scrap,
            ItemType::Scrap,
            ItemType::Scrap,
        ] {
            let a = self.rng.range(0.0, TAU);
            let sp = self.rng.range(60.0, 160.0);
            let it = self.new_item(
                kind,
                mx + a.cos() * 40.0,
                my + a.sin() * 40.0,
                r.b.vx + a.cos() * sp,
                r.b.vy + a.sin() * sp,
            );
            self.items.push(it);
        }
        self.explosion_fx(mx, my, 200.0, Tint::Danger);
        self.shake = 18.0;
        self.emit(Event::HitStop(HITSTOP_BOSS));
        self.sfx(Sfx::Boom);
        self.emit(Event::Highlight {
            score: 1500.0,
            text: "Wrecked the Kessler jaws".to_owned(),
        });
        self.popup(mx, my - 40.0, "JAWS WRECKED".to_owned(), Tint::Danger);
        self.radio(Radio::JawsWrecked, true, false);
    }

    /// The jaws pull everything toward the maw and eat what reaches it. The
    /// pull is an acceleration, so a tug hauling heavy cargo must drag all
    /// that mass out against it.
    fn feed_jaws(&mut self, dt: f64) {
        let r = self.refinery.clone().expect("refinery");
        let (mx, my) = r.maw();
        let strength = JAW_PULL * r.hunger;
        let pull_r = std::cell::Cell::new(r.pull_r);
        let pull = |b: &mut Body, pr: f64| {
            let (dx, dy) = (mx - b.x, my - b.y);
            let d = js::hypot(dx, dy);
            if d < pr && d > 1.0 {
                let a = strength * (1.0 - d / pr) * dt;
                b.vx += (dx / d) * a;
                b.vy += (dy / d) * a;
            }
            d
        };
        // eat(): the jaws grow as they feed.
        let mut eats: Vec<(f64, f64, Tint)> = Vec::new();
        let eat = |x: f64, y: f64, t: Tint, eats: &mut Vec<(f64, f64, Tint)>| {
            eats.push((x, y, t));
            let n = r.eaten + eats.len() as u32;
            pull_r.set(JAW_RANGE_MAX.min(JAW_RANGE_START + f64::from(n) * 10.0));
        };
        for i in 0..self.rocks.len() {
            let rk = &mut self.rocks[i];
            if !rk.dead && pull(&mut rk.b, pull_r.get()) < MAW_R + rk.b.r * 0.4 {
                rk.dead = true;
                eat(rk.b.x, rk.b.y, Tint::Rock, &mut eats);
            }
        }
        for w in &mut self.wrecks {
            if !w.dead && pull(&mut w.b, pull_r.get()) < MAW_R + w.b.r * 0.3 {
                w.dead = true;
                eat(w.b.x, w.b.y, Tint::Wreck, &mut eats);
            }
        }
        let mut i = 0;
        while i < self.items.len() {
            let it = &mut self.items[i];
            if it.dead || pull(&mut it.b, pull_r.get()) >= MAW_R + it.b.r * 0.5 {
                i += 1;
                continue;
            }
            let (x, y, towed, value, volatile) =
                (it.b.x, it.b.y, it.towed, it.value, it.def().volatile);
            if towed {
                self.stats.lost += value;
                self.radio(Radio::AteCargo, true, false);
            }
            self.remove_item(i);
            eat(x, y, Tint::Ore, &mut eats);
            // Something volatile goes off in the maw and hurts it.
            if volatile {
                self.refinery.as_mut().expect("refinery").pull_r = pull_r.get();
                self.hurt_jaws(JAW_FEED_DAMAGE);
                self.explosion_fx(x, y, 90.0, Tint::Reactor);
            }
            i += 1;
        }
        for j in &mut self.jumpers {
            if !j.dead && pull(&mut j.b, pull_r.get()) < MAW_R {
                j.dead = true;
                eat(j.b.x, j.b.y, Tint::Jumper, &mut eats);
            }
        }
        // Jaw kills aren't the player's doing: no wanted level.
        for c in 0..self.collectors.len() {
            let cl = &mut self.collectors[c];
            if !cl.dead && pull(&mut cl.b, pull_r.get()) < MAW_R {
                cl.by_jaws = true;
                let (x, y) = (cl.b.x, cl.b.y);
                self.hurt_collector(c, f64::INFINITY, None);
                eat(x, y, Tint::Collector, &mut eats);
            }
        }
        for k in 0..self.sheriffs.len() {
            let sh = &mut self.sheriffs[k];
            if !sh.dead && pull(&mut sh.b, pull_r.get()) < MAW_R {
                sh.by_jaws = true;
                let (x, y) = (sh.b.x, sh.b.y);
                self.hurt_sheriff(k, f64::INFINITY, None);
                eat(x, y, Tint::Law, &mut eats);
            }
        }
        for m in &mut self.miners {
            if !m.dead && pull(&mut m.b, pull_r.get()) < MAW_R {
                m.dead = true;
                eat(m.b.x, m.b.y, Tint::Miner, &mut eats);
            }
        }
        for c in &mut self.crabs {
            if c.dead || c.state != CrabState::Free {
                continue;
            }
            let mut b = Body {
                x: c.x,
                y: c.y,
                vx: c.vx,
                vy: c.vy,
                r: c.r,
                mass: 1.0,
                fixed: false,
            };
            let d = pull(&mut b, pull_r.get());
            c.vx = b.vx;
            c.vy = b.vy;
            if d < MAW_R {
                c.dead = true;
                eat(c.x, c.y, Tint::Crab, &mut eats);
            }
        }
        if self.ship.alive {
            let d = pull(&mut self.ship.b, pull_r.get());
            if d < MAW_R + self.ship.b.r {
                self.damage_ship(45.0 * dt);
                self.radio(Radio::JawsShip, true, true);
            }
        }
        if let Some(r) = self.refinery.as_mut() {
            r.eaten += eats.len() as u32;
            if !eats.is_empty() {
                r.chomp = 0.25;
                r.pull_r = pull_r.get();
            }
        }
        for (x, y, t) in eats {
            self.burst(x, y, 12, t, 180.0);
            self.sfx(Sfx::Crunch);
        }
    }

    /* --------------------------- Security drones --------------------------- */

    fn spawn_drone(&mut self) {
        let r = self.refinery.as_ref().expect("refinery").b;
        let a = self.rng.range(0.0, TAU);
        let fire_t = self.rng.range(1.2, 2.2);
        let id = self.fresh_id();
        self.drones.push(Drone {
            id,
            b: Body {
                x: r.x + a.cos() * (REFINERY_HUB_R + 40.0),
                y: r.y + a.sin() * (REFINERY_HUB_R + 40.0),
                vx: r.vx,
                vy: r.vy,
                r: 13.0,
                mass: 1.5,
                fixed: false,
            },
            hp: 3.0,
            angle: 0.0,
            orbit: a,
            fire_t,
            flash: 0.0,
            dead: false,
        });
    }

    /// Advances refinery security drone `i` one step: orbiting the station and attacking.
    pub(crate) fn update_drone(&mut self, i: usize, dt: f64) {
        let r = self.refinery.as_ref().map(|r| r.b);
        let s = self.ship.b;
        let d = &mut self.drones[i];
        d.flash = (d.flash - dt).max(0.0);
        d.fire_t -= dt;
        if let Some(r) = r {
            d.orbit += 0.45 * dt;
            let (tx, ty) = (r.x + d.orbit.cos() * 420.0, r.y + d.orbit.sin() * 420.0);
            steer(&mut d.b, &mut d.angle, tx, ty, 220.0, 260.0, dt);
        }
        if !self.ship.alive {
            return;
        }
        let d = &mut self.drones[i];
        d.angle = (s.y - d.b.y).atan2(s.x - d.b.x);
        let dist = d.b.dist_to(&s);
        if d.fire_t <= 0.0 && dist < 800.0 {
            d.fire_t = self.rng.range(1.6, 2.4);
            let lead = dist / 480.0;
            let db = d.b;
            let a = (s.y + s.vy * lead - db.y).atan2(s.x + s.vx * lead - db.x)
                + self.rng.range(-0.1, 0.1);
            self.ebullets.push(Bullet {
                x: db.x,
                y: db.y,
                vx: a.cos() * 480.0,
                vy: a.sin() * 480.0,
                life: 1.7,
                dmg: EBULLET_DAMAGE,
                ..Bullet::default()
            });
            self.sfx(Sfx::EnemyShot(dist.min(65535.0) as u16));
            self.under_attack();
        }
    }

    /// Damages refinery drone `i` by `n`; a dead drone is ignored.
    pub fn hurt_drone(&mut self, i: usize, n: f64) {
        let d = &mut self.drones[i];
        if d.dead {
            return;
        }
        d.hp -= n;
        d.flash = 0.1;
        if d.hp > 0.0 {
            return;
        }
        d.dead = true;
        let b = d.b;
        let vx = b.vx + self.rng.range(-50.0, 50.0);
        let vy = b.vy + self.rng.range(-50.0, 50.0);
        let it = self.new_item(ItemType::Scrap, b.x, b.y, vx, vy);
        self.items.push(it);
        self.explosion_fx(b.x, b.y, 70.0, Tint::Danger);
        self.sfx(Sfx::Boom);
        self.count_kill(|k| k.drone += 1);
        self.radio(Radio::DroneKill, false, false);
    }

    /* --------------------------- Debt collectors --------------------------- */

    /// Is this shift's installment at risk? Collectors only net you when
    /// it is (paymentAtRisk).
    pub fn payment_at_risk(&self) -> bool {
        let Some(c) = self.career.as_ref() else {
            return false;
        };
        let due = c.due(0);
        let have = c.wallet + self.banked;
        if have >= due {
            return false;
        }
        // Good standing buys time; bad standing wants a safety margin.
        let trust = self.standing_bank();
        if trust == Tier::Good {
            return self.time_left < 45.0;
        }
        if self.time_left < 60.0 {
            return true;
        }
        let elapsed = self.t.max(30.0);
        let projected = have + (self.banked / elapsed) * self.time_left + self.line_value() * 0.5;
        projected < due * if trust == Tier::Bad { 1.25 } else { 1.0 }
    }

    /// Spawns a debt collector just outside a field edge, at least 1,300 px from the tug (when a spot that far exists).
    pub fn spawn_collector(&mut self) {
        let (px, py) = self.edge_point(1300.0);
        let id = self.fresh_id();
        self.collectors.push(Collector {
            id,
            b: Body {
                x: px.clamp(40.0, WORLD_W - 40.0),
                y: py.clamp(40.0, WORLD_H - 40.0),
                vx: 0.0,
                vy: 0.0,
                r: 22.0,
                mass: COLLECTOR_MASS,
                fixed: false,
            },
            hp: COLLECTOR_HP,
            angle: 0.0,
            state: CollectorState::Approach,
            fire_t: 3.0,
            tether: 0.0,
            exit: (0.0, 0.0),
            recover_t: 0.0,
            flash: 0.0,
            edge_warned: false,
            greedy: false,
            greed_roll_t: 1.0,
            calm_t: 0.0,
            provoked: false,
            by_jaws: false,
            dead: false,
        });
        self.radio(Radio::Collector, true, false);
        self.sfx(Sfx::Siren);
    }

    /// Finds a collector by its stable id; `None` if it has been swept.
    pub fn collector_index(&self, id: u32) -> Option<usize> {
        self.collectors.iter().position(|c| c.id == id)
    }

    /// A greedy collector got you off the field: the whole load is stolen
    /// with no credit to your loan; the tug is released.
    fn jack_haul(&mut self, c: usize) {
        let value = self.line_value();
        self.drop_chain_into(Tint::Collector);
        self.stats.lost += value;
        let exit = self.collectors[c].exit;
        let s = &mut self.ship;
        s.net = None;
        s.b.vx = -exit.0 * 240.0;
        s.b.vy = -exit.1 * 240.0;
        s.invuln = 2.0;
        let (x, y) = (s.b.x, s.b.y);
        self.collectors[c].dead = true;
        let text = if value > 0.0 {
            format!("HAUL JACKED · −{} CR", fmt_credits(value))
        } else {
            "NOTHING TO TAKE".to_owned()
        };
        self.popup(x, y - 34.0, text, Tint::Collector);
        self.sfx(Sfx::Confiscated);
        let text = if value > 0.0 {
            format!(
                "They jacked the whole haul. {} cr of cargo, and not a credit off your loan. Greedy bastards.",
                fmt_credits(value)
            )
        } else {
            "They dragged you out for nothing. Your line was empty. Ha.".to_owned()
        };
        self.radio_text(Radio::Jacked, true, false, text);
    }

    /// Remove every piece on the line, with a burst each (seizures).
    fn drop_chain_into(&mut self, tint: Tint) {
        let ids = self.chain.clone();
        for id in ids {
            if let Some(i) = self.item_index(id) {
                let (x, y) = (self.items[i].b.x, self.items[i].b.y);
                self.burst(
                    x,
                    y,
                    if tint == Tint::Law { 10 } else { 12 },
                    tint,
                    if tint == Tint::Law { 140.0 } else { 160.0 },
                );
                self.remove_item(i);
            }
        }
    }

    /// Advances collector `i` one step according to its `CollectorState`.
    pub(crate) fn update_collector(&mut self, i: usize, dt: f64) {
        let s = self.ship.b;
        let alive = self.ship.alive;
        {
            let c = &mut self.collectors[i];
            c.flash = (c.flash - dt).max(0.0);
            c.fire_t -= dt;
        }
        let id = self.collectors[i].id;
        match self.collectors[i].state {
            CollectorState::Tow => {
                let holding = alive && self.ship.net.is_some_and(|n| n.by == id);
                let c = &mut self.collectors[i];
                if !holding {
                    c.state = CollectorState::Recover;
                    c.recover_t = 5.0;
                    return;
                }
                c.b.vx += c.exit.0 * COLLECTOR_HAUL_ACCEL * dt;
                c.b.vy += c.exit.1 * COLLECTOR_HAUL_ACCEL * dt;
                c.angle = c.exit.1.atan2(c.exit.0);
                // Distance from the tug to the edge it's dragged toward.
                let e = c.exit;
                let edge = if e.0 < 0.0 {
                    s.x
                } else if e.0 > 0.0 {
                    WORLD_W - s.x
                } else if e.1 < 0.0 {
                    s.y
                } else {
                    WORLD_H - s.y
                };
                if !c.edge_warned && edge < 500.0 {
                    c.edge_warned = true;
                    self.radio(Radio::NetEdge, true, false);
                }
                // Off the field: a greedy crew steals the load; a fat load is
                // taken as credit; otherwise the tug is impounded.
                if edge < -60.0 {
                    if self.collectors[i].greedy {
                        self.jack_haul(i);
                    } else if self.line_value() >= FAT_CABLE_VALUE {
                        self.confiscate(i);
                    } else {
                        self.impound(i);
                    }
                }
            }
            CollectorState::Recover => {
                let c = &mut self.collectors[i];
                c.recover_t -= dt;
                if alive {
                    let a = (c.b.y - s.y).atan2(c.b.x - s.x);
                    c.b.vx += a.cos() * 120.0 * dt;
                    c.b.vy += a.sin() * 120.0 * dt;
                }
                if c.recover_t <= 0.0 {
                    c.state = CollectorState::Approach;
                    c.fire_t = 2.5;
                }
            }
            CollectorState::Leave => {
                let c = &mut self.collectors[i];
                let e = nearest_exit(c.b.x, c.b.y);
                c.b.vx += e.0 * 240.0 * dt;
                c.b.vy += e.1 * 240.0 * dt;
                c.angle = c.b.vy.atan2(c.b.vx);
                if out_of_field(c.b.x, c.b.y, 120.0) {
                    c.dead = true;
                }
            }
            CollectorState::Approach => {
                if !alive {
                    return;
                }
                let at_risk = self.payment_at_risk();
                let line = self.line_value();
                let bad_bank = self.standing_bank() == Tier::Bad;
                let c = &mut self.collectors[i];
                let d = nonzero(c.b.dist_to(&s));
                // With the payment covered, a collector only shadows you.
                let hold = if at_risk || c.greedy {
                    COLLECTOR_HOLD
                } else {
                    COLLECTOR_SHADOW
                };
                let (tx, ty) = (
                    s.x + ((c.b.x - s.x) / d) * hold,
                    s.y + ((c.b.y - s.y) / d) * hold,
                );
                steer(&mut c.b, &mut c.angle, tx, ty, 260.0, 220.0, dt);
                c.angle = (s.y - c.b.y).atan2(s.x - c.b.x);
                if !at_risk && !c.greedy {
                    // Rolling each second for greed while there's a haul...
                    c.greed_roll_t -= dt;
                    let mut went_greedy = false;
                    if c.greed_roll_t <= 0.0 {
                        c.greed_roll_t = 1.0;
                        let chance = COLLECTOR_GREED_CHANCE * if bad_bank { 3.0 } else { 1.0 };
                        if line > 0.0 && self.rng.next() < chance {
                            let c = &mut self.collectors[i];
                            c.greedy = true;
                            c.fire_t = c.fire_t.min(1.0);
                            went_greedy = true;
                        }
                    }
                    if went_greedy {
                        self.radio(Radio::Greedy, true, false);
                    }
                    // ...and leaving once it has watched long enough.
                    let c = &mut self.collectors[i];
                    c.calm_t += dt;
                    if !c.greedy && c.calm_t > COLLECTOR_SHADOW_TIME {
                        c.state = CollectorState::Leave;
                        self.radio(Radio::CollectorLeaves, false, false);
                    }
                    return;
                }
                c.calm_t = 0.0;
                // The carrier is private property.
                let sanctuary = js::dist(s.x, s.y, BAY_X, BAY_Y) < BAY_SANCTUARY;
                if c.fire_t <= 0.0
                    && d < COLLECTOR_FIRE_RANGE
                    && !sanctuary
                    && self.ship.net.is_none()
                {
                    c.fire_t = 3.5;
                    // Lead the shot; nets are slow, so a moving tug can dodge.
                    let lead = d / NET_SPEED;
                    let a =
                        (s.y + s.vy * lead * 0.8 - c.b.y).atan2(s.x + s.vx * lead * 0.8 - c.b.x);
                    let cb = c.b;
                    let nid = self.fresh_id();
                    self.nets.push(Net {
                        id: nid,
                        x: cb.x + a.cos() * 26.0,
                        y: cb.y + a.sin() * 26.0,
                        vx: cb.vx + a.cos() * NET_SPEED,
                        vy: cb.vy + a.sin() * NET_SPEED,
                        life: NET_LIFE,
                        spin: 0.0,
                        owner: id,
                    });
                    self.sfx(Sfx::NetFire);
                    self.under_attack();
                }
            }
        }
    }

    /// Fly nets: they catch the tug, snag on rocks, and ignore cargo.
    pub(crate) fn update_nets(&mut self, dt: f64) {
        for k in 0..self.nets.len() {
            let n = &mut self.nets[k];
            n.x += n.vx * dt;
            n.y += n.vy * dt;
            n.life -= dt;
            n.spin += dt * 4.0;
            if n.life <= 0.0 {
                continue;
            }
            let s = &self.ship;
            if s.alive
                && s.net.is_none()
                && s.invuln <= 0.0
                && js::dist(n.x, n.y, s.b.x, s.b.y) < s.b.r + NET_RADIUS
            {
                n.life = 0.0;
                let owner = n.owner;
                if let Some(c) = self
                    .collector_index(owner)
                    .filter(|&c| !self.collectors[c].dead)
                {
                    self.net_ship(c);
                }
                continue;
            }
            let (nx, ny) = (n.x, n.y);
            if self
                .rocks
                .iter()
                .any(|r| !r.dead && js::dist(nx, ny, r.b.x, r.b.y) < r.b.r + NET_RADIUS * 0.5)
            {
                self.nets[k].life = 0.0;
                self.burst(nx, ny, 8, Tint::Collector, 90.0);
            }
        }
    }

    /// Collector `c`'s net has landed: the tug is netted and held by that collector.
    pub fn net_ship(&mut self, c: usize) {
        let s = self.ship.b;
        let cl = &mut self.collectors[c];
        self.ship.net = Some(NetHold {
            by: cl.id,
            hp: NET_HP,
        });
        self.ship.last_turn_sign = 0.0;
        cl.state = CollectorState::Tow;
        cl.tether = cl.b.dist_to(&s) + 5.0;
        cl.exit = nearest_exit(cl.b.x, cl.b.y);
        cl.edge_warned = false;
        self.shake = 8.0;
        self.burst(s.x, s.y, 16, Tint::Collector, 140.0);
        self.sfx(Sfx::Netted);
        // Let the "uh-oh" finish before the netted warble starts.
        self.ship.alarm_t = 1.2;
        self.emit(Event::Rumble {
            strength: 0.6,
            ms: 300.0,
        });
        self.radio(Radio::Netted, true, false);
    }

    /// Release the tug from a net; `torn` means the player broke out.
    pub(crate) fn free_ship(&mut self, torn: bool) {
        let by = self.ship.net.take().map(|n| n.by);
        if let Some(c) = by.and_then(|id| self.collector_index(id)) {
            let cl = &mut self.collectors[c];
            if !cl.dead {
                cl.state = CollectorState::Recover;
                cl.recover_t = 5.0;
            }
        }
        if torn {
            let (x, y) = (self.ship.b.x, self.ship.b.y);
            self.burst(x, y, 24, Tint::Collector, 220.0);
            self.popup(x, y - 30.0, "TORN FREE".to_owned(), Tint::Collector);
            self.sfx(Sfx::TornFree);
            self.radio(Radio::NetFree, true, false);
        }
    }

    /// Damages collector `c` by `n`; a dead one is ignored. `by_item` is set when cargo did the damage, and a value of 3 or more counts as a smash. Destroying a collector costs standing with the bank and earns a wanted star.
    pub fn hurt_collector(&mut self, c: usize, n: f64, by_item: Option<f64>) {
        let cl = &mut self.collectors[c];
        if cl.dead {
            return;
        }
        cl.hp -= n;
        cl.flash = 0.1;
        if cl.hp > 0.0 {
            return;
        }
        cl.dead = true;
        let (b, id, by_jaws) = (cl.b, cl.id, cl.by_jaws);
        if self.ship.net.is_some_and(|h| h.by == id) {
            self.free_ship(false);
        }
        // Repo-ship armour is good salvage.
        for _ in 0..2 {
            let x = b.x + self.rng.range(-12.0, 12.0);
            let y = b.y + self.rng.range(-12.0, 12.0);
            let vx = b.vx + self.rng.range(-60.0, 60.0);
            let vy = b.vy + self.rng.range(-60.0, 60.0);
            let it = self.new_item(ItemType::Plate, x, y, vx, vy);
            self.items.push(it);
        }
        let vx = b.vx + self.rng.range(-60.0, 60.0);
        let vy = b.vy + self.rng.range(-60.0, 60.0);
        let it = self.new_item(ItemType::Scrap, b.x, b.y, vx, vy);
        self.items.push(it);
        self.explosion_fx(b.x, b.y, 115.0, Tint::Collector);
        self.sfx(Sfx::Boom);
        if by_jaws || self.no_blame {
            return;
        }
        self.kills.collector += 1;
        // Destroying a debt collector is a crime: one wanted star.
        self.rep_delta.bank += REP_COLLECTOR_KILL_BANK;
        self.add_wanted(1, b.x, b.y);
        if by_item.is_some_and(|m| m >= 3.0) {
            self.popup(b.x, b.y - 26.0, "SMASHED".to_owned(), Tint::Cable);
            self.radio(Radio::Smash, true, false);
            self.emit(Event::HitStop(HITSTOP_SMASH));
            self.emit(Event::Highlight {
                score: 800.0,
                text: "Smashed a debt collector with cargo".to_owned(),
            });
        } else {
            let line = if self.wanted > 1 {
                Radio::WantedMore
            } else {
                Radio::CollectorKill
            };
            self.radio(line, true, false);
        }
    }

    /// The collector got a tug with a fat cable off the field: the whole
    /// load is taken as payment (credited at CONFISCATE_RATE).
    fn confiscate(&mut self, c: usize) {
        let value = self.line_value();
        let credit = js::round(value * CONFISCATE_RATE / 10.0) * 10.0;
        self.drop_chain_into(Tint::Collector);
        if let Some(car) = self.career.as_mut() {
            car.loan = (car.loan - credit).max(0.0);
        }
        self.confiscated += value;
        self.loan_credit += credit;
        let exit = self.collectors[c].exit;
        let s = &mut self.ship;
        s.net = None;
        s.b.vx = -exit.0 * 240.0;
        s.b.vy = -exit.1 * 240.0;
        s.invuln = 2.0;
        let (x, y) = (s.b.x, s.b.y);
        self.collectors[c].dead = true; // leaves with the goods
        let back = self.t + self.rng.range(COLLECTOR_RETURN.0, COLLECTOR_RETURN.1);
        self.collector_queue.push(back);
        self.popup(
            x,
            y - 34.0,
            format!("CONFISCATED · −{} LOAN", fmt_credits(credit)),
            Tint::Collector,
        );
        self.sfx(Sfx::Confiscated);
        let text = format!(
            "They took the whole load as \"payment\". {} cr of cargo, credited at {}. Liquidation prices. Thieves with paperwork, sugar.",
            fmt_credits(value),
            fmt_credits(credit)
        );
        self.radio_text(Radio::Confiscated, true, false, text);
    }

    /// The collector got the tug off the field: impound fee, shift over.
    fn impound(&mut self, c: usize) {
        self.sfx(Sfx::SadTrombone);
        let fee = self.banked.min(js::round(
            IMPOUND_FEE * self.diff_fees * self.crew_fee_mul(),
        ));
        self.banked -= fee;
        self.impounded = true;
        self.rep_delta.bank += REP_IMPOUNDED_BANK;
        self.impound_fee = fee;
        self.collectors[c].dead = true;
        self.ship.net = None;
        self.shift_over = true;
    }

    /* -------------------------------- The law ------------------------------- */

    /// Add wanted stars (capped); crime gets a response soon after.
    pub fn add_wanted(&mut self, n: u32, x: f64, y: f64) {
        let before = self.wanted;
        self.wanted = (self.wanted + n).min(WANTED_MAX);
        if self.wanted > before {
            self.rep_delta.law += REP_STAR_GAINED_LAW * f64::from(self.wanted - before);
            self.popup(
                x,
                y - 48.0,
                format!("WANTED {}", "★".repeat(self.wanted as usize)),
                Tint::Law,
            );
            self.sfx(Sfx::WantedUp);
        }
        let soon = self.rng.range(6.0, 12.0);
        self.reinforce_t = Some(self.reinforce_t.unwrap_or(10.0).min(soon));
        if self.wanted == WANTED_MAX && before < WANTED_MAX {
            self.radio(Radio::Swat, true, false);
            self.emit(Event::Unlock("fiveStar"));
        }
    }

    /// Patrols come round more often when the law dislikes you.
    fn next_patrol_delay(&mut self) -> f64 {
        let bad = self.standing_law() == Tier::Bad;
        self.rng.range(90.0, 150.0) * self.cond.def().patrol * if bad { 0.7 } else { 1.0 }
    }

    /// Spawn a law unit; `responding` units come straight for you.
    pub fn spawn_sheriff(&mut self, responding: bool) {
        let t = law_tier(self.wanted.max(1)).expect("tier");
        let (px, py) = self.edge_point(1200.0);
        let id = self.fresh_id();
        self.sheriffs.push(Sheriff {
            id,
            b: Body {
                x: px.clamp(40.0, WORLD_W - 40.0),
                y: py.clamp(40.0, WORLD_H - 40.0),
                vx: 0.0,
                vy: 0.0,
                r: if t.swat { 24.0 } else { 20.0 },
                mass: if t.swat { 7.0 } else { 5.0 },
                fixed: false,
            },
            hp: t.hp,
            angle: 0.0,
            state: if responding {
                SheriffState::Pursuit
            } else {
                SheriffState::Patrol
            },
            wp: None,
            patrol_t: SHERIFF_PATROL_TIME,
            fire_t: 1.5,
            cuff: 0.0,
            lost_t: 0.0,
            siren_t: 0.0,
            flash: 0.0,
            speed: t.speed,
            fire: t.fire,
            dmg: t.dmg,
            stun: t.stun,
            cuff_time: t.cuff,
            swat: t.swat,
            shakedown: false,
            shake_roll_t: 1.0,
            warned: false,
            provoked: false,
            by_jaws: false,
            dead: false,
        });
        if responding {
            let line = if t.swat {
                Radio::Swat
            } else if self.sheriffs.len() > 1 {
                Radio::Reinforcements
            } else {
                Radio::SheriffWanted
            };
            self.radio(line, true, false);
        } else {
            self.radio(Radio::SheriffPatrol, false, false);
        }
    }

    fn start_pursuit(&mut self, k: usize) {
        let sh = &mut self.sheriffs[k];
        if sh.state == SheriffState::Pursuit {
            return;
        }
        sh.state = SheriffState::Pursuit;
        sh.lost_t = 0.0;
        sh.cuff = 0.0;
        let line = if sh.shakedown {
            Radio::Shakedown
        } else {
            Radio::Pursuit
        };
        self.radio(line, true, false);
    }

    /// Units still in the field and not leaving: the current response.
    pub fn active_law(&self) -> usize {
        self.sheriffs
            .iter()
            .filter(|s| !s.dead && s.state != SheriffState::Leave)
            .count()
    }

    /// A wrecked tug, or reaching the bay, wipes the slate: no stars, every
    /// unit leaves, reinforcements are cancelled.
    pub(crate) fn clear_record(&mut self) {
        let was = self.wanted;
        self.wanted = 0;
        self.reinforce_t = None;
        for sh in &mut self.sheriffs {
            if !sh.dead {
                sh.state = SheriffState::Leave;
                sh.shakedown = false;
                sh.cuff = 0.0;
            }
        }
        if was > 0 {
            let (x, y) = (self.ship.b.x, self.ship.b.y);
            self.popup(x, y - 52.0, "WANTED CLEARED".to_owned(), Tint::Law);
        }
    }

    /// Advances law unit `k` one step according to its `SheriffState`.
    pub(crate) fn update_sheriff(&mut self, k: usize, dt: f64) {
        let s = self.ship.b;
        let alive = self.ship.alive;
        let sh = &mut self.sheriffs[k];
        sh.flash = (sh.flash - dt).max(0.0);
        sh.fire_t -= dt;
        let d = if alive {
            sh.b.dist_to(&s)
        } else {
            f64::INFINITY
        };
        if sh.state == SheriffState::Patrol
            && self.wanted > 0
            && alive
            && d < detect_range(self.wanted)
        {
            self.start_pursuit(k);
        }
        // A clean tug hauling cargo near a patrol: each second, a small
        // chance he decides to "inspect" the cargo, i.e. steal it.
        let line = self.line_value();
        let sh = &mut self.sheriffs[k];
        if sh.state == SheriffState::Patrol
            && self.wanted == 0
            && alive
            && d < detect_range(1)
            && line > 0.0
        {
            sh.shake_roll_t -= dt;
            if sh.shake_roll_t <= 0.0 {
                sh.shake_roll_t = 1.0;
                let law = match self.standing_law() {
                    Tier::Good => 0.0,
                    Tier::Bad => 2.0,
                    Tier::Neutral => 1.0,
                };
                if self.rng.next() < SHAKEDOWN_CHANCE * law * self.cond.def().shakedown {
                    self.sheriffs[k].shakedown = true;
                    self.start_pursuit(k);
                }
            }
        }
        let sh = &mut self.sheriffs[k];
        match sh.state {
            SheriffState::Pursuit => {
                // Checked after the shakedown roll, so a roll that just
                // started a chase isn't cancelled in the same frame.
                let target = alive && (self.wanted > 0 || sh.shakedown);
                if !target {
                    sh.state = SheriffState::Patrol;
                    sh.cuff = 0.0;
                    sh.shakedown = false;
                    sh.patrol_t = sh.patrol_t.min(20.0);
                    return;
                }
                // Chase, aiming a little ahead of the tug.
                let lead = (d / 600.0).min(1.0) * 0.5;
                let speed = sh.speed;
                steer(
                    &mut sh.b,
                    &mut sh.angle,
                    s.x + s.vx * lead,
                    s.y + s.vy * lead,
                    speed,
                    480.0,
                    dt,
                );
                sh.angle = (s.y - sh.b.y).atan2(s.x - sh.b.x);
                sh.siren_t -= dt;
                let siren = sh.siren_t <= 0.0;
                if siren {
                    sh.siren_t = 0.75;
                    self.sfx(Sfx::PoliceSiren(d.min(65535.0) as u16));
                }
                // Bolts: stun bolts slow the tug; SWAT fires real rounds.
                let sh = &mut self.sheriffs[k];
                if sh.fire_t <= 0.0 && d < 600.0 {
                    let (f0, f1) = sh.fire;
                    let fire_t = self.rng.range(f0, f1);
                    let sh = &mut self.sheriffs[k];
                    sh.fire_t = fire_t;
                    let t = d / 560.0;
                    let a = (s.y + s.vy * t - sh.b.y).atan2(s.x + s.vx * t - sh.b.x);
                    let (b, stun, swat, dmg) = (sh.b, sh.stun, sh.swat, sh.dmg);
                    let a = a + self.rng.range(-0.08, 0.08);
                    self.ebullets.push(Bullet {
                        x: b.x,
                        y: b.y,
                        vx: a.cos() * 560.0,
                        vy: a.sin() * 560.0,
                        life: 1.4,
                        dmg,
                        stun,
                        swat,
                        ..Bullet::default()
                    });
                    self.sfx(Sfx::StunShot(d.min(65535.0) as u16));
                    self.under_attack();
                }
                // Cuffing: stay out of reach. The meter drains once clear.
                let sh = &mut self.sheriffs[k];
                if d < SHERIFF_CUFF_RANGE {
                    sh.cuff += dt;
                    if sh.cuff >= sh.cuff_time {
                        self.arrest(k);
                        return;
                    }
                } else {
                    sh.cuff = (sh.cuff - dt * 0.5).max(0.0);
                }
                // Losing him: far enough away for long enough.
                let sh = &mut self.sheriffs[k];
                if d > detect_range(self.wanted.max(1)) * SHERIFF_LOSE_MULT {
                    sh.lost_t += dt;
                    if sh.lost_t > SHERIFF_LOSE_TIME {
                        sh.state = SheriffState::Patrol;
                        sh.cuff = 0.0;
                        sh.shakedown = false;
                        self.radio(Radio::LostSheriff, true, false);
                    }
                } else {
                    sh.lost_t = 0.0;
                }
            }
            SheriffState::Leave => {
                let e = nearest_exit(sh.b.x, sh.b.y);
                sh.b.vx += e.0 * 320.0 * dt;
                sh.b.vy += e.1 * 320.0 * dt;
                sh.angle = sh.b.vy.atan2(sh.b.vx);
                if out_of_field(sh.b.x, sh.b.y, 120.0) {
                    sh.dead = true;
                    self.patrol_t = self.next_patrol_delay();
                }
            }
            SheriffState::Patrol => {
                // Cruise between random waypoints, giving a clean tug a wide berth.
                sh.patrol_t -= dt;
                let need_wp = sh
                    .wp
                    .is_none_or(|(wx, wy)| js::dist(wx, wy, sh.b.x, sh.b.y) < 120.0);
                if need_wp {
                    let wx = self.rng.range(400.0, WORLD_W - 400.0);
                    let wy = self.rng.range(400.0, WORLD_H - 400.0);
                    self.sheriffs[k].wp = Some((wx, wy));
                }
                let sh = &mut self.sheriffs[k];
                if alive && d < 260.0 {
                    let away = (sh.b.y - s.y).atan2(sh.b.x - s.x);
                    let (tx, ty) = (sh.b.x + away.cos() * 300.0, sh.b.y + away.sin() * 300.0);
                    steer(&mut sh.b, &mut sh.angle, tx, ty, 260.0, 400.0, dt);
                } else {
                    let (wx, wy) = sh.wp.expect("waypoint");
                    steer(&mut sh.b, &mut sh.angle, wx, wy, 220.0, 200.0, dt);
                }
                if sh.patrol_t <= 0.0 {
                    sh.state = SheriffState::Leave;
                }
            }
        }
    }

    /// Damage to a law unit. Hitting him with a clean record earns a
    /// warning; destroying him is a crime: two stars, and the next patrol
    /// comes in fast.
    pub fn hurt_sheriff(&mut self, k: usize, n: f64, by_item: Option<f64>) {
        let sh = &mut self.sheriffs[k];
        if sh.dead {
            return;
        }
        sh.hp -= n;
        sh.flash = 0.1;
        let warn = !sh.by_jaws && !self.no_blame && !sh.warned && self.wanted == 0;
        if warn {
            sh.warned = true;
            self.radio(Radio::SheriffShotAt, true, false);
        }
        let sh = &mut self.sheriffs[k];
        if sh.hp > 0.0 {
            if self.wanted > 0 && !self.no_blame {
                self.start_pursuit(k);
            }
            return;
        }
        sh.dead = true;
        let (b, by_jaws) = (sh.b, sh.by_jaws);
        for _ in 0..2 {
            let x = b.x + self.rng.range(-12.0, 12.0);
            let y = b.y + self.rng.range(-12.0, 12.0);
            let vx = b.vx + self.rng.range(-60.0, 60.0);
            let vy = b.vy + self.rng.range(-60.0, 60.0);
            let it = self.new_item(ItemType::Plate, x, y, vx, vy);
            self.items.push(it);
        }
        self.explosion_fx(b.x, b.y, 120.0, Tint::Law);
        self.sfx(Sfx::Boom);
        // The department sends someone else, quickly.
        self.patrol_t = self.rng.range(25.0, 45.0);
        if by_jaws || self.no_blame {
            return;
        }
        self.kills.law += 1;
        self.rep_delta.law += REP_LAW_KILL_LAW;
        self.add_wanted(2, b.x, b.y);
        if by_item.is_some_and(|m| m >= 3.0) {
            self.popup(b.x, b.y - 26.0, "SMASHED".to_owned(), Tint::Cable);
            self.emit(Event::HitStop(HITSTOP_SMASH));
            self.emit(Event::Highlight {
                score: 900.0,
                text: "Smashed a law cruiser with cargo".to_owned(),
            });
        }
        self.radio(Radio::SheriffKill, true, false);
    }

    /// Arrested. The fine (FINE_PER_STAR per star) comes out of the cargo
    /// on your line first, then this shift's earnings; the record resets
    /// and the tug goes back to the bay. The shift goes on.
    fn arrest(&mut self, k: usize) {
        // Every unit stands down once someone has you.
        for u in &mut self.sheriffs {
            u.state = SheriffState::Leave;
            u.cuff = 0.0;
        }
        if self.sheriffs[k].shakedown {
            self.shakedown_take(k);
            return;
        }
        let stars = self.wanted;
        let mul = match self.standing_law() {
            Tier::Good => 0.7,
            Tier::Bad => 1.3,
            Tier::Neutral => 1.0,
        };
        let fine = js::round(FINE_PER_STAR * f64::from(stars) * mul);
        self.rep_delta.law += REP_ARRESTED_LAW;
        let cargo = self.line_value();
        self.drop_chain_into(Tint::Law);
        let from_cargo = fine.min(cargo);
        let from_earnings = self.banked.min(fine - from_cargo);
        self.banked -= from_earnings;
        self.fines += from_cargo + from_earnings;
        self.fine_from_cargo += from_cargo;
        self.seized += cargo;
        self.arrests += 1;
        self.wanted = 0;
        if self.ship.net.is_some() {
            self.free_ship(false);
        }
        let s = &mut self.ship;
        s.b.x = BAY_X;
        s.b.y = BAY_Y;
        s.b.vx = 0.0;
        s.b.vy = 0.0;
        s.angle = PI;
        s.av = 0.0;
        s.invuln = 2.0;
        s.stun_t = 0.0;
        self.sfx(Sfx::Arrest);
        let paid = if cargo <= 0.0 {
            format!("{} cr out of your earnings", fmt_credits(from_earnings))
        } else if from_earnings > 0.0 {
            format!(
                "your cargo covered {} cr and {} cr came out of your earnings",
                fmt_credits(from_cargo),
                fmt_credits(from_earnings)
            )
        } else if cargo > fine {
            "your cargo covered it, and they kept the change".to_owned()
        } else {
            "your cargo covered it".to_owned()
        };
        let text = format!(
            "Arrested. {stars}-star fine of {} cr: {paid}. Your record is clean. My nerves are not.",
            fmt_credits(fine)
        );
        self.radio_text(Radio::Arrested, true, false, text);
    }

    /// A crooked shakedown: no fine, no stars; he takes the cargo on your
    /// line and lets you go where you are.
    fn shakedown_take(&mut self, k: usize) {
        let cargo = self.line_value();
        self.drop_chain_into(Tint::Law);
        self.stats.lost += cargo;
        self.sheriffs[k].shakedown = false;
        let s = &mut self.ship;
        s.stun_t = 0.0;
        s.invuln = 2.0;
        let (x, y) = (s.b.x, s.b.y);
        self.sfx(Sfx::Arrest);
        self.popup(
            x,
            y - 34.0,
            format!("SHAKEN DOWN · −{} CR", fmt_credits(cargo)),
            Tint::Law,
        );
        let text = format!(
            "He took {} cr of cargo \"as evidence.\" No charge, no receipt. That is not how evidence works, hotshot.",
            fmt_credits(cargo)
        );
        self.radio_text(Radio::ShookDown, true, false, text);
    }

    /* --------------------------- Missiles --------------------------- */

    /// Hostiles the missiles (and sentries) engage: never rocks, crabs or
    /// calm miners. Collectors and the law only once you've hit them.
    pub fn missile_targets(&self) -> Vec<Tgt> {
        let mut v = Vec::new();
        v.extend(
            self.jumpers
                .iter()
                .filter(|j| !j.dead)
                .map(|j| Tgt::Jumper(j.id)),
        );
        v.extend(
            self.drones
                .iter()
                .filter(|d| !d.dead)
                .map(|d| Tgt::Drone(d.id)),
        );
        v.extend(
            self.collectors
                .iter()
                .filter(|c| c.provoked && !c.dead)
                .map(|c| Tgt::Collector(c.id)),
        );
        v.extend(
            self.sheriffs
                .iter()
                .filter(|s| s.provoked && !s.dead)
                .map(|s| Tgt::Sheriff(s.id)),
        );
        v.extend(
            self.miners
                .iter()
                .filter(|m| m.angry_t > 0.0 && !m.dead)
                .map(|m| Tgt::Miner(m.id)),
        );
        v.extend(
            self.sentries
                .iter()
                .filter(|d| d.rogue && !d.dead)
                .map(|d| Tgt::Sentry(d.id)),
        );
        // Awake, feeding jaws are fair game.
        if self
            .refinery
            .as_ref()
            .is_some_and(|r| r.state == RefineryState::Hungry)
        {
            v.push(Tgt::Jaws);
        }
        v
    }

    /// A target's position, velocity and radius, if it's still alive.
    pub fn target_body(&self, t: Tgt) -> Option<Body> {
        match t {
            Tgt::Net(id) => self
                .nets
                .iter()
                .find(|n| n.id == id && n.life > 0.0)
                .map(|n| Body {
                    x: n.x,
                    y: n.y,
                    vx: n.vx,
                    vy: n.vy,
                    r: 0.0,
                    mass: 1.0,
                    fixed: false,
                }),
            Tgt::Jumper(id) => self
                .jumpers
                .iter()
                .find(|j| j.id == id && !j.dead)
                .map(|j| j.b),
            Tgt::Drone(id) => self
                .drones
                .iter()
                .find(|d| d.id == id && !d.dead)
                .map(|d| d.b),
            Tgt::Collector(id) => self
                .collectors
                .iter()
                .find(|c| c.id == id && !c.dead)
                .map(|c| c.b),
            Tgt::Sheriff(id) => self
                .sheriffs
                .iter()
                .find(|s| s.id == id && !s.dead)
                .map(|s| s.b),
            Tgt::Miner(id) => self
                .miners
                .iter()
                .find(|m| m.id == id && !m.dead)
                .map(|m| m.b),
            Tgt::Sentry(id) => self
                .sentries
                .iter()
                .find(|d| d.id == id && !d.dead)
                .map(|d| Body {
                    x: d.x,
                    y: d.y,
                    vx: d.vx,
                    vy: d.vy,
                    r: d.r,
                    mass: 1.0,
                    fixed: false,
                }),
            Tgt::Jaws => self
                .refinery
                .as_ref()
                .filter(|r| r.state != RefineryState::Wrecked)
                .map(|r| r.jaw_body()),
        }
    }

    /// Damage whichever hostile `t` is.
    pub fn hurt_target(&mut self, t: Tgt, n: f64) {
        match t {
            Tgt::Net(id) => {
                if let Some(net) = self.nets.iter_mut().find(|x| x.id == id) {
                    net.life = 0.0;
                }
            }
            Tgt::Jumper(id) => {
                if let Some(i) = self.jumper_index(id) {
                    self.hurt_jumper(i, n, None);
                }
            }
            Tgt::Drone(id) => {
                if let Some(i) = self.drones.iter().position(|d| d.id == id) {
                    self.hurt_drone(i, n);
                }
            }
            Tgt::Collector(id) => {
                if let Some(i) = self.collector_index(id) {
                    self.hurt_collector(i, n, None);
                }
            }
            Tgt::Sheriff(id) => {
                if let Some(i) = self.sheriffs.iter().position(|s| s.id == id) {
                    self.hurt_sheriff(i, n, None);
                }
            }
            Tgt::Miner(id) => {
                if let Some(i) = self.miners.iter().position(|m| m.id == id) {
                    self.hurt_miner(i, n, true);
                }
            }
            Tgt::Sentry(id) => {
                if let Some(i) = self.sentries.iter().position(|d| d.id == id) {
                    self.hurt_sentry(i, n);
                }
            }
            Tgt::Jaws => self.hurt_jaws(n),
        }
    }

    /// True while a missile in flight already chases `t`.
    fn claimed(&self, t: Tgt) -> bool {
        self.missiles
            .iter()
            .any(|m| m.life > 0.0 && m.target == Some(t))
    }

    /// Nets on their way in come first, then the nearest hostile in range.
    fn pick_missile_target(&self, x: f64, y: f64) -> Option<Tgt> {
        let mut best = None;
        let mut best_d = MISSILE_NET_RANGE;
        for n in &self.nets {
            if n.life <= 0.0 || self.claimed(Tgt::Net(n.id)) {
                continue;
            }
            let d = js::dist(n.x, n.y, x, y);
            if d < best_d {
                best = Some(Tgt::Net(n.id));
                best_d = d;
            }
        }
        if best.is_some() {
            return best;
        }
        best_d = MISSILE_RANGE;
        for t in self.missile_targets() {
            if self.claimed(t) {
                continue;
            }
            let Some(b) = self.target_body(t) else {
                continue;
            };
            let d = js::dist(b.x, b.y, x, y);
            if d < best_d {
                best = Some(t);
                best_d = d;
            }
        }
        best
    }

    /// Reloads and launches the tug's missiles, then flies the ones in the air.
    pub fn update_missiles(&mut self, dt: f64) {
        // Launching: reload the rack, then fire if a target is in range.
        if self.mods.missiles && self.ship.alive {
            let m = &self.mods;
            let s = &mut self.ship;
            if s.missiles < m.missile_rack {
                s.missile_reload_t -= dt;
                if s.missile_reload_t <= 0.0 {
                    s.missiles += 1;
                    s.missile_reload_t = m.missile_reload;
                }
            }
            if s.missiles >= m.missile_rack {
                s.missile_reload_t = m.missile_reload;
            }
            s.missile_gap_t -= dt;
            if s.missiles > 0 && s.missile_gap_t <= 0.0 {
                let sb = s.b;
                if let Some(first) = self.pick_missile_target(sb.x, sb.y) {
                    self.ship.missile_gap_t = MISSILE_GAP;
                    // A salvo spreads over different targets when there are several.
                    let salvo = self.mods.missile_salvo.min(self.ship.missiles);
                    for i in 0..salvo {
                        let target = if i == 0 {
                            first
                        } else {
                            self.pick_missile_target(sb.x, sb.y).unwrap_or(first)
                        };
                        let s = &mut self.ship;
                        s.missiles -= 1;
                        // Launch from the pods, kicked out sideways.
                        let side = if s.missiles % 2 == 1 { 1.0 } else { -1.0 };
                        let (px, py) = (-s.angle.sin() * side, s.angle.cos() * side);
                        let torpedo = self.mods.torpedo;
                        self.missiles.push(Missile {
                            x: s.b.x + px * 14.0,
                            y: s.b.y + py * 14.0,
                            vx: s.b.vx + px * 160.0,
                            vy: s.b.vy + py * 160.0,
                            angle: s.angle,
                            target: Some(target),
                            life: MISSILE_LIFE * if torpedo { 1.5 } else { 1.0 },
                            torpedo,
                        });
                    }
                    self.sfx(Sfx::Missile);
                    self.radio(Radio::MissilesLive, false, true);
                }
            }
        }

        // Flight: steer toward the target; if it's gone, find another.
        for k in 0..self.missiles.len() {
            if self.missiles[k].life <= 0.0 {
                continue;
            }
            self.missiles[k].life -= dt;
            if self.missiles[k].life <= 0.0 {
                continue;
            }
            let alive_t = self.missiles[k]
                .target
                .and_then(|t| self.target_body(t).map(|_| t));
            if alive_t.is_none() {
                let (mx, my) = (self.missiles[k].x, self.missiles[k].y);
                self.missiles[k].target = if self.ship.alive {
                    self.pick_missile_target(mx, my)
                } else {
                    None
                };
            }
            let tb = self.missiles[k].target.and_then(|t| self.target_body(t));
            let m = &mut self.missiles[k];
            if let Some(tb) = tb {
                // Blend the velocity toward "full speed at the target":
                // plain acceleration overshoots and orbits.
                let (dx, dy) = (tb.x - m.x, tb.y - m.y);
                let d = nonzero(js::hypot(dx, dy));
                let tm = if m.torpedo { TORPEDO_SPEED } else { 1.0 };
                let kk = (MISSILE_TURN * tm * dt).min(1.0);
                m.vx += ((dx / d) * MISSILE_SPEED * tm - m.vx) * kk;
                m.vy += ((dy / d) * MISSILE_SPEED * tm - m.vy) * kk;
            }
            m.x += m.vx * dt;
            m.y += m.vy * dt;
            m.angle = m.vy.atan2(m.vx);
            // (The exhaust trail is drawn by the game.)
            // Impact: the target, or any rock in the way.
            let (mx, my, tgt) = (m.x, m.y, m.target);
            let hit = tgt.and_then(|t| self.target_body(t).map(|b| (t, b)));
            if let Some((t, tb)) = hit {
                let r = if matches!(t, Tgt::Net(_)) {
                    NET_RADIUS
                } else {
                    tb.r
                };
                if js::dist(tb.x, tb.y, mx, my) < r + 6.0 {
                    if let Tgt::Net(_) = t {
                        self.hurt_target(t, 0.0);
                        self.burst(tb.x, tb.y, 14, Tint::Collector, 160.0);
                    } else {
                        let dmg = self.mods.missile_dmg;
                        self.hurt_target(t, dmg);
                    }
                    if self.missiles[k].torpedo {
                        self.torpedo_blast(k, t);
                    }
                    self.missile_blast(k);
                    continue;
                }
            }
            if let Some(r) = (0..self.rocks.len()).find(|&r| {
                let rk = &self.rocks[r];
                !rk.dead
                    && (rk.b.x - mx).abs() <= rk.b.r
                    && js::dist(rk.b.x, rk.b.y, mx, my) <= rk.b.r
            }) {
                self.damage_rock(r, 1.0);
                self.missile_blast(k);
            }
        }
    }

    /// A torpedo's blast hits every engaged hostile nearby (not the direct
    /// target again) and chips rocks.
    fn torpedo_blast(&mut self, k: usize, direct: Tgt) {
        let (mx, my) = (self.missiles[k].x, self.missiles[k].y);
        for t in self.missile_targets() {
            if t == direct {
                continue;
            }
            let Some(b) = self.target_body(t) else {
                continue;
            };
            let d = js::dist(b.x, b.y, mx, my) - b.r;
            if d >= TORPEDO_SPLASH {
                continue;
            }
            self.hurt_target(
                t,
                TORPEDO_SPLASH_DAMAGE * (1.0 - d.max(0.0) / TORPEDO_SPLASH),
            );
        }
        let mut r = 0;
        while r < self.rocks.len() {
            let rk = &self.rocks[r];
            if !rk.dead && js::dist(rk.b.x, rk.b.y, mx, my) < TORPEDO_SPLASH + rk.b.r {
                self.damage_rock(r, 2.0);
            }
            r += 1;
        }
        self.explosion_fx(mx, my, TORPEDO_SPLASH, Tint::Hazard);
        self.shake = (self.shake + 6.0).min(14.0);
        self.sfx(Sfx::Boom);
    }

    fn missile_blast(&mut self, k: usize) {
        let m = &mut self.missiles[k];
        m.life = 0.0;
        let (x, y) = (m.x, m.y);
        self.explosion_fx(x, y, 45.0, Tint::Hazard);
        self.sfx(Sfx::MissileHit);
    }

    /* ------------------------------ Sentries ------------------------------ */

    /// A sentry drone launched from the bay (makeSentry).
    pub fn make_sentry(&mut self, i: usize, n: usize) -> Sentry {
        let s = self.ship.b;
        let a = i as f64 / n.max(1) as f64 * TAU;
        let fire_t = self.rng.range(0.2, self.mods.drone_fire);
        let id = self.fresh_id();
        Sentry {
            id,
            x: s.x + a.cos() * SENTRY_ORBIT,
            y: s.y + a.sin() * SENTRY_ORBIT,
            vx: 0.0,
            vy: 0.0,
            r: 9.0,
            hp: self.mods.drone_hp,
            orbit: a,
            fire_t,
            angle: a,
            flash: 0.0,
            rogue: false,
            wander: (0.0, 0.0),
            dead: false,
        }
    }

    /// The tug is gone: every surviving sentry goes rogue.
    pub(crate) fn go_rogue(&mut self) {
        let loose: Vec<usize> = (0..self.sentries.len())
            .filter(|&i| !self.sentries[i].dead && !self.sentries[i].rogue)
            .collect();
        if loose.is_empty() {
            return;
        }
        for i in loose {
            let a = self.rng.range(0.0, TAU);
            let wx = self.rng.range(300.0, WORLD_W - 300.0);
            let wy = self.rng.range(300.0, WORLD_H - 300.0);
            let fire_t = self.rng.range(1.0, 2.0);
            let d = &mut self.sentries[i];
            d.rogue = true;
            d.vx = a.cos() * ROGUE_SPEED;
            d.vy = a.sin() * ROGUE_SPEED;
            d.wander = (wx, wy);
            d.fire_t = fire_t;
        }
        self.radio(Radio::SentriesRogue, true, false);
    }

    /// What a rogue shoots at: the tug and every crew in the field (not
    /// other drones, or orphans would just shoot each other).
    fn rogue_targets(&self) -> Vec<(Option<Tgt>, Body)> {
        let mut v = Vec::new();
        if self.ship.alive {
            v.push((None, self.ship.b));
        }
        v.extend(
            self.jumpers
                .iter()
                .filter(|j| !j.dead)
                .map(|j| (Some(Tgt::Jumper(j.id)), j.b)),
        );
        v.extend(
            self.collectors
                .iter()
                .filter(|c| !c.dead)
                .map(|c| (Some(Tgt::Collector(c.id)), c.b)),
        );
        v.extend(
            self.sheriffs
                .iter()
                .filter(|s| !s.dead)
                .map(|s| (Some(Tgt::Sheriff(s.id)), s.b)),
        );
        v.extend(
            self.miners
                .iter()
                .filter(|m| !m.dead)
                .map(|m| (Some(Tgt::Miner(m.id)), m.b)),
        );
        v.extend(
            self.drones
                .iter()
                .filter(|d| !d.dead)
                .map(|d| (Some(Tgt::Drone(d.id)), d.b)),
        );
        v
    }

    /// A rogue's bolt hit something other than the tug. Real damage, but
    /// not yours: no grudge, no stars, no angry miners.
    fn rogue_hit(&mut self, t: Tgt, n: f64) {
        self.no_blame = true;
        match t {
            Tgt::Miner(id) => {
                if let Some(i) = self.miners.iter().position(|m| m.id == id) {
                    self.hurt_miner(i, n, false);
                }
            }
            _ => self.hurt_target(t, n),
        }
        self.no_blame = false;
    }

    /// What a rogue's bolt at (x, y) strikes, other than the tug.
    pub(crate) fn rogue_bolt_target(&self, x: f64, y: f64) -> Option<Tgt> {
        self.rogue_targets()
            .into_iter()
            .find_map(|(t, b)| t.filter(|_| js::dist(x, y, b.x, b.y) <= b.r + 3.0))
    }

    /// A rogue drone's bolt hit target `t`; it does twice `SENTRY_DAMAGE`.
    pub(crate) fn rogue_bolt_hit(&mut self, t: Tgt) {
        self.rogue_hit(t, SENTRY_DAMAGE * 2.0);
    }

    fn update_rogue(&mut self, i: usize, dt: f64) {
        self.sentries[i].fire_t -= dt;
        let d = &self.sentries[i];
        if js::dist(d.wander.0, d.wander.1, d.x, d.y) < 120.0 {
            let w = (
                self.rng.range(300.0, WORLD_W - 300.0),
                self.rng.range(300.0, WORLD_H - 300.0),
            );
            self.sentries[i].wander = w;
        }
        let d = &self.sentries[i];
        let mut target = None;
        let mut best = ROGUE_RANGE;
        for (t, b) in self.rogue_targets() {
            let dd = js::dist(b.x, b.y, d.x, d.y);
            if dd < best {
                best = dd;
                target = Some((t, b));
            }
        }
        // Close in on a target to about 220 px; otherwise keep roaming.
        let goal = match target {
            Some((_, b)) => {
                let bb = nonzero(best);
                (
                    b.x + (d.x - b.x) / bb * 220.0,
                    b.y + (d.y - b.y) / bb * 220.0,
                )
            }
            None => d.wander,
        };
        let d = &mut self.sentries[i];
        let a = (goal.1 - d.y).atan2(goal.0 - d.x);
        let k = (1.5 * dt).min(1.0);
        d.vx += (a.cos() * ROGUE_SPEED - d.vx) * k;
        d.vy += (a.sin() * ROGUE_SPEED - d.vy) * k;
        d.x = (d.x + d.vx * dt).clamp(20.0, WORLD_W - 20.0);
        d.y = (d.y + d.vy * dt).clamp(20.0, WORLD_H - 20.0);
        let Some((_, tb)) = target else {
            d.angle = d.vy.atan2(d.vx);
            return;
        };
        let sp = 520.0;
        let lead = best / sp;
        let (dx, dy) = (d.x, d.y);
        let aim =
            (tb.y + tb.vy * lead - dy).atan2(tb.x + tb.vx * lead - dx) + self.rng.range(-0.1, 0.1);
        let d = &mut self.sentries[i];
        d.angle = aim;
        if d.fire_t <= 0.0 {
            d.fire_t = ROGUE_FIRE;
            let id = d.id;
            self.ebullets.push(Bullet {
                x: dx,
                y: dy,
                vx: aim.cos() * sp,
                vy: aim.sin() * sp,
                life: 1.2,
                dmg: ROGUE_DAMAGE,
                rogue: true,
                owner: Some(id),
                ..Bullet::default()
            });
            self.sfx(Sfx::SentryShot);
        }
    }

    /// Advances sentry drone `i` one step; a rogue one wanders and hunts instead of orbiting.
    pub(crate) fn update_sentry(&mut self, i: usize, dt: f64) {
        let s = self.ship.b;
        let d = &mut self.sentries[i];
        d.flash = (d.flash - dt).max(0.0);
        if d.rogue {
            self.update_rogue(i, dt);
            return;
        }
        d.fire_t -= dt;
        d.orbit += dt * 1.3;
        // Ease toward the orbit slot; with the tug wrecked, hold position.
        if self.ship.alive {
            let (tx, ty) = (
                s.x + d.orbit.cos() * SENTRY_ORBIT,
                s.y + d.orbit.sin() * SENTRY_ORBIT,
            );
            let k = (9.0 * dt).min(1.0);
            d.x += (tx - d.x) * k;
            d.y += (ty - d.y) * k;
        }
        let (dx, dy) = (d.x, d.y);
        let mut target = None;
        let mut best = SENTRY_RANGE;
        // Sentry bolts pass through the law, so never aim at it.
        for t in self.missile_targets() {
            if matches!(t, Tgt::Sheriff(_)) {
                continue;
            }
            let Some(b) = self.target_body(t) else {
                continue;
            };
            let dd = js::dist(b.x, b.y, dx, dy);
            if dd < best {
                best = dd;
                target = Some(b);
            }
        }
        let Some(tb) = target else { return };
        // Lead the target: aim where it will be when the shot arrives.
        let sp = 800.0;
        let t = best / sp;
        let d = &mut self.sentries[i];
        d.angle = (tb.y + tb.vy * t - dy).atan2(tb.x + tb.vx * t - dx);
        if d.fire_t <= 0.0 {
            d.fire_t = self.mods.drone_fire;
            let a = d.angle;
            self.bullets.push(Bullet {
                x: dx,
                y: dy,
                vx: a.cos() * sp,
                vy: a.sin() * sp,
                life: 0.7,
                dmg: self.mods.drone_dmg,
                sentry: true,
                ..Bullet::default()
            });
            self.sfx(Sfx::SentryShot);
        }
    }

    /// Damages sentry drone `i` by `n`; a dead one is ignored.
    pub fn hurt_sentry(&mut self, i: usize, n: f64) {
        let d = &mut self.sentries[i];
        if d.dead {
            return;
        }
        d.hp -= n;
        d.flash = 0.12;
        if d.hp > 0.0 {
            return;
        }
        d.dead = true;
        let (x, y, rogue) = (d.x, d.y, d.rogue);
        self.explosion_fx(x, y, 40.0, Tint::Cable);
        self.sfx(Sfx::MissileHit);
        let (vx, vy) = (self.rng.range(-40.0, 40.0), self.rng.range(-40.0, 40.0));
        let it = self.new_item(ItemType::Scrap, x, y, vx, vy);
        self.items.push(it);
        if rogue {
            // A rogue leaves its core behind: valuable, volatile, still angry.
            let (vx, vy) = (self.rng.range(-30.0, 30.0), self.rng.range(-30.0, 30.0));
            let it = self.new_item(ItemType::Core, x, y, vx, vy);
            self.items.push(it);
            self.radio(Radio::RogueDown, false, false);
            if !self.no_blame {
                self.emit(Event::Unlock("rogue"));
            }
            self.count_kill(|k| k.rogue += 1);
            return;
        }
        let left = self.sentries.iter().filter(|x| !x.dead && !x.rogue).count();
        let text = format!("Lost a sentry drone. {left} left. Those are not free, hotshot.");
        self.radio_text(Radio::SentryLost, true, false, text);
    }

    /* ---------------------------- Mining drones ---------------------------- */

    /// One work crew, somewhere away from the carrier.
    pub fn spawn_miners(&mut self, count: i32) {
        let (mut cx, mut cy) = (CARRIER_X, CARRIER_Y);
        for _ in 0..30 {
            cx = self.rng.range(500.0, WORLD_W - 500.0);
            cy = self.rng.range(500.0, WORLD_H - 500.0);
            if js::dist(cx, cy, CARRIER_X, CARRIER_Y) > 1300.0 {
                break;
            }
        }
        for _ in 0..count {
            let x = cx + self.rng.range(-220.0, 220.0);
            let y = cy + self.rng.range(-220.0, 220.0);
            let angle = self.rng.range(0.0, TAU);
            let retarget = self.rng.range(0.0, 1.0);
            let id = self.fresh_id();
            self.miners.push(Miner {
                id,
                b: Body {
                    x,
                    y,
                    vx: 0.0,
                    vy: 0.0,
                    r: 14.0,
                    mass: 2.0,
                    fixed: false,
                },
                hp: MINER_HP,
                angle,
                target: None,
                beam: None,
                drill: 0.0,
                angry_t: 0.0,
                retarget,
                zap_t: 0.0,
                flash: 0.0,
                dead: false,
            });
        }
    }

    /// The nearest rock of size 2 or more; pebbles are left as debris.
    fn pick_miner_target(&self, m: &Body) -> Option<u32> {
        let mut best = None;
        let mut best_d = 2600.0;
        for r in &self.rocks {
            if r.dead || r.size < 2 {
                continue;
            }
            let d = r.b.dist_to(m);
            if d < best_d {
                best = Some(r.id);
                best_d = d;
            }
        }
        best
    }

    /// Advances mining-crew tug `i` one step: cutting its rock, or attacking the tug while angry.
    pub(crate) fn update_miner(&mut self, i: usize, dt: f64) {
        let s = self.ship.b;
        let m = &mut self.miners[i];
        m.flash = (m.flash - dt).max(0.0);
        m.angry_t = (m.angry_t - dt).max(0.0);
        m.retarget -= dt;
        m.beam = None;

        // Retaliation: hold close to the tug and cut it.
        if m.angry_t > 0.0 && self.ship.alive {
            let d = nonzero(m.b.dist_to(&s));
            let (tx, ty) = (
                s.x + ((m.b.x - s.x) / d) * 180.0,
                s.y + ((m.b.y - s.y) / d) * 180.0,
            );
            steer(&mut m.b, &mut m.angle, tx, ty, 240.0, 300.0, dt);
            m.angle = (s.y - m.b.y).atan2(s.x - m.b.x);
            if d < MINER_ANGRY_RANGE {
                m.beam = Some((s.x, s.y));
                m.zap_t -= dt;
                let zap = m.zap_t <= 0.0;
                if zap {
                    m.zap_t = 0.13;
                }
                self.under_attack();
                self.damage_ship(MINER_BEAM_DPS * dt);
                if zap {
                    self.sfx(Sfx::BeamZap);
                }
            }
            return;
        }

        let rocks = &self.rocks;
        let stale = m.target.is_none_or(|id| {
            rocks
                .iter()
                .position(|r| r.id == id)
                .is_none_or(|r| rocks[r].dead)
        });
        if m.retarget <= 0.0 || stale {
            let mb = m.b;
            let t = self.pick_miner_target(&mb);
            let m = &mut self.miners[i];
            m.target = t;
            m.retarget = 2.0;
        }
        let ri = self.miners[i]
            .target
            .and_then(|id| self.rocks.iter().position(|r| r.id == id));
        let m = &mut self.miners[i];
        let Some(ri) = ri else {
            m.b.vx *= 0.98;
            m.b.vy *= 0.98;
            return;
        };
        let r = self.rocks[ri].b;
        // Park on the near side of the rock at a fixed standoff.
        let d = nonzero(m.b.dist_to(&r));
        let (ux, uy) = ((m.b.x - r.x) / d, (m.b.y - r.y) / d);
        let (tx, ty) = (
            r.x + ux * (r.r + MINER_STANDOFF),
            r.y + uy * (r.r + MINER_STANDOFF),
        );
        steer(&mut m.b, &mut m.angle, tx, ty, 200.0, 260.0, dt);
        m.angle = (r.y - m.b.y).atan2(r.x - m.b.x);
        if d - r.r < MINER_BEAM_RANGE {
            m.beam = Some((r.x + ux * r.r, r.y + uy * r.r));
            m.drill += MINER_DRILL_RATE * dt;
            while self.miners[i].drill >= 1.0 && !self.rocks[ri].dead {
                self.miners[i].drill -= 1.0;
                self.damage_rock_by(ri, 1.0, true);
            }
            if self.rocks[ri].dead {
                let m = &mut self.miners[i];
                m.drill = 0.0;
                m.retarget = 0.0;
            }
            // First time the tug wanders into a working crew, warn of the storm.
            if self.ship.alive && s.dist_to(&self.miners[i].b) < 700.0 {
                self.radio(Radio::Storm, false, true);
            }
        }
    }

    /// Damage a mining drone. Only a provoked hit makes it beam you back,
    /// and only for MINER_ANGRY_TIME; the crew doesn't join in.
    pub fn hurt_miner(&mut self, i: usize, n: f64, provoked: bool) {
        let m = &mut self.miners[i];
        if m.dead {
            return;
        }
        m.hp -= n;
        m.flash = 0.1;
        if m.hp > 0.0 {
            if !provoked {
                return;
            }
            let first = m.angry_t <= 0.0;
            m.angry_t = MINER_ANGRY_TIME;
            if first {
                self.radio(Radio::MinerAngry, true, false);
            }
            return;
        }
        m.dead = true;
        let b = m.b;
        // A burst ore hopper: the reward for taking one out.
        for _ in 0..self.rng.int(2, 3) {
            let x = b.x + self.rng.range(-10.0, 10.0);
            let y = b.y + self.rng.range(-10.0, 10.0);
            let vx = b.vx + self.rng.range(-70.0, 70.0);
            let vy = b.vy + self.rng.range(-70.0, 70.0);
            let it = self.new_item(ItemType::Ore, x, y, vx, vy);
            self.items.push(it);
        }
        let vx = b.vx + self.rng.range(-50.0, 50.0);
        let vy = b.vy + self.rng.range(-50.0, 50.0);
        let it = self.new_item(ItemType::Scrap, b.x, b.y, vx, vy);
        self.items.push(it);
        self.explosion_fx(b.x, b.y, 75.0, Tint::Miner);
        self.sfx(Sfx::Boom);
        self.count_kill(|k| k.miner += 1);
        self.radio(Radio::MinerKill, false, false);
    }

    /* ------------------------- Shift setup and spawns ------------------------- */

    /// Collector waves, the refinery, patrols and the mining crew for a new
    /// shift (the opposition part of newGame).
    pub(crate) fn setup_opposition(&mut self) {
        let mode = self
            .career
            .as_ref()
            .map(|c| c.mode)
            .unwrap_or(crate::career::Mode::Standard);
        let d = mode.def();
        let shift = self.shift_no;
        // Collector waves this shift: none before the mode's first
        // collector shift, one on that shift, then up to the mode's limit.
        let waves = if shift < d.collector_from {
            0
        } else {
            (d.collector_waves.min(shift - d.collector_from + 1) + self.belt.def().collectors)
                .max(0)
        };
        let first = if shift == 1 {
            self.rng.range(150.0, 200.0)
        } else {
            self.rng.range(90.0, 150.0)
        };
        self.collector_queue = (0..waves).map(|i| first + f64::from(i) * 60.0).collect();
        // Shift 1 lets the player learn towing before the refinery shows up.
        self.refinery_t = if shift == 1 { 75.0 } else { REFINERY_ARRIVE_AT };
        self.patrol_t = self.rng.range(60.0, 140.0) * self.cond.def().patrol;
        self.miner_announce_t = 8.0;
        self.collector_from = d.collector_from;
        self.net_tear = d.net_tear;
        // Sentries launch from the drone bay at the start of the shift.
        let n = self.career.as_ref().map_or(0, |c| c.sentries as usize);
        if self.mods.drone_bay {
            for i in 0..n {
                let s = self.make_sentry(i, n);
                self.sentries.push(s);
            }
        }
    }

    /// The mining crew joins after the field is seeded (end of newGame).
    pub(crate) fn setup_miners(&mut self) {
        let mode = self
            .career
            .as_ref()
            .map(|c| c.mode)
            .unwrap_or(crate::career::Mode::Standard);
        let n = (miner_crew_for(self.shift_no) + mode.def().miner_crew).max(1);
        self.spawn_miners(n);
    }

    /// The opposition's arrivals (the first half of updateSpawns).
    pub(crate) fn update_opposition_spawns(&mut self, dt: f64) {
        if self.miner_announce_t > 0.0 {
            self.miner_announce_t -= dt;
            if self.miner_announce_t <= 0.0 && !self.miners.is_empty() {
                self.radio(Radio::Miners, true, false);
            }
        }
        self.collector_queue.sort_by(|a, b| a.total_cmp(b));
        while self.collector_queue.first().is_some_and(|&t| self.t >= t) {
            self.collector_queue.remove(0);
            if self.collectors.len() < COLLECTOR_MAX && self.ship.alive {
                self.spawn_collector();
            }
        }

        // The law. Wanted: units are dispatched until the response for your
        // star level is out. Clean: one random patrol at a time.
        // Lawless belts have no law at all: stars pile up, but nobody comes.
        let lawless = self.belt.def().lawless;
        match law_tier(self.wanted).filter(|_| !lawless) {
            Some(t) if self.ship.alive => {
                let r = self.reinforce_t.unwrap_or(0.0) - dt;
                self.reinforce_t = Some(r);
                if self.active_law() < t.units && r <= 0.0 {
                    self.spawn_sheriff(true);
                    self.reinforce_t = Some(if t.swat { 2.0 } else { 4.0 });
                }
            }
            _ => {
                if !lawless && self.sheriffs.is_empty() && self.ship.alive {
                    self.patrol_t -= dt;
                    if self.patrol_t <= 0.0 {
                        self.spawn_sheriff(false);
                    }
                }
            }
        }

        // Money-driven triggers, once the mode's collectors have started.
        if self.shift_no >= self.collector_from {
            if !self.low_wallet_checked && self.t >= LOW_WALLET_CHECK_AT {
                self.low_wallet_checked = true;
                let short = self
                    .career
                    .as_ref()
                    .map_or(0.0, |c| c.due(0) - (c.wallet + self.banked));
                if short > 0.0 && self.collectors.is_empty() {
                    self.collector_queue.push(self.t + 6.0);
                    let v = fmt_credits(short);
                    let text = format!(
                        "Kessler Mutual ran your numbers, hon. You're {v} cr short of this shift's installment, so they're sending somebody. Bank something. Today."
                    );
                    self.radio_text(Radio::LowWallet, true, false, text);
                }
            }
            // Fat cable: a rich load draws a collector (once a minute at most).
            let soon = self.collector_queue.iter().any(|t| t - self.t < 20.0);
            if self.t >= self.fat_cable_ready_at
                && self.collectors.is_empty()
                && !soon
                && self.line_value() >= FAT_CABLE_VALUE
            {
                self.fat_cable_ready_at = self.t + FAT_CABLE_COOLDOWN;
                self.collector_queue.push(self.t + 8.0);
                self.radio(Radio::FatCable, true, false);
            }
        }
        if self.refinery.is_none() && self.refinery_t > 0.0 {
            self.refinery_t -= dt;
            if self.refinery_t <= 0.0 {
                self.spawn_refinery();
            }
        }
    }
}

/// The browser's `x || 1`.
fn nonzero(x: f64) -> f64 {
    if x == 0.0 {
        1.0
    } else {
        x
    }
}
