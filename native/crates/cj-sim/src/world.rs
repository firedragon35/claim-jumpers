//! The shift in progress (the browser's global `G`) and its fixed step.
//!
//! Update order follows step() in src/js/16-simulation-step.js. Systems
//! that arrive in later milestones are noted where they slot in, so the
//! order stays faithful as they are added.

use crate::consts::*;
use crate::entities::*;
use crate::events::{Event, Radio, Sfx, Tint};
use crate::input::{Action, Input};
use crate::items::ItemType;
use crate::js;
use crate::mods::Mods;
use crate::rng::Rng;

/// Camera centre in world units, and zoom (screen px per world unit).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub x: f64,
    pub y: f64,
    pub zoom: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stats {
    pub items: u32,
    /// Largest single delivery (bankings within DELIVERY_WINDOW of each other).
    pub best_run: f64,
    pub deaths: u32,
}

pub struct World {
    pub rng: Rng,
    /// Seconds simulated this shift.
    pub t: f64,
    pub time_left: f64,
    /// Credits banked this shift.
    pub banked: f64,
    pub shift_no: i32,
    pub mods: Mods,
    /// Difficulty multipliers (Standard mode is 1 and 1).
    pub diff_damage: f64,
    pub diff_fees: f64,
    /// The belt's price multiplier (1 in the starting belt).
    pub belt_value: f64,
    pub stats: Stats,
    pub delivery_total: f64,
    pub delivery_t: f64,
    pub ship: Ship,
    /// Item ids from the tug outward: chain[0] hangs off the tug.
    pub chain: Vec<u32>,
    pub items: Vec<Item>,
    pub rocks: Vec<Rock>,
    pub wrecks: Vec<Wreck>,
    pub next_id: u32,
    pub cam: Camera,
    /// Viewport size in logical pixels; the camera's zoom depends on it.
    pub view: (f64, f64),
    /// Screen shake amplitude and white flash, both decaying each step.
    pub shake: f64,
    pub screen_flash: f64,
    warned_minute: bool,
    warned_ten: bool,
    /// The shift clock reached zero. The end-of-shift report arrives with
    /// the career in milestone 3; until then the game shows a placeholder.
    pub shift_over: bool,
    /// Effects, sounds and radio lines raised since the last drain.
    pub events: Vec<Event>,
}

impl World {
    /// An empty field with a fresh stock tug at the bay.
    pub fn empty(rng: Rng, view: (f64, f64)) -> World {
        let mods = Mods::stock();
        World {
            rng,
            t: 0.0,
            time_left: SHIFT_SECONDS,
            banked: 0.0,
            shift_no: 1,
            ship: Ship::new(&mods),
            mods,
            diff_damage: 1.0,
            diff_fees: 1.0,
            belt_value: 1.0,
            stats: Stats::default(),
            delivery_total: 0.0,
            delivery_t: -99.0,
            chain: Vec::new(),
            items: Vec::new(),
            rocks: Vec::new(),
            wrecks: Vec::new(),
            next_id: 1,
            cam: Camera {
                x: BAY_X,
                y: BAY_Y,
                zoom: 1.0,
            },
            view,
            shake: 0.0,
            screen_flash: 0.0,
            warned_minute: false,
            warned_ten: false,
            shift_over: false,
            events: Vec::new(),
        }
    }

    /// A new shift: the field is seeded and Marge opens the channel.
    pub fn new_shift(rng: Rng, view: (f64, f64)) -> World {
        let mut w = World::empty(rng, view);
        w.populate();
        w.radio(Radio::Start, true, false);
        w
    }

    pub(crate) fn emit(&mut self, e: Event) {
        self.events.push(e);
    }
    pub(crate) fn sfx(&mut self, s: Sfx) {
        self.emit(Event::Sfx(s));
    }
    pub(crate) fn radio(&mut self, key: Radio, force: bool, once: bool) {
        self.emit(Event::Radio { key, force, once });
    }
    pub(crate) fn popup(&mut self, x: f64, y: f64, text: String, tint: Tint) {
        self.emit(Event::Popup { x, y, text, tint });
    }

    /// Create an item with a fresh id (not yet added to the field).
    pub fn new_item(&mut self, kind: ItemType, x: f64, y: f64, vx: f64, vy: f64) -> Item {
        let id = self.next_id;
        self.next_id += 1;
        make_item(&mut self.rng, id, kind, x, y, vx, vy, self.belt_value)
    }

    pub fn item_index(&self, id: u32) -> Option<usize> {
        self.items.iter().position(|it| it.id == id)
    }

    /// True when a point is clear of the carrier's safe zone and existing bodies.
    fn clear_spot(&self, x: f64, y: f64, r: f64, safe: f64) -> bool {
        if js::dist(x, y, CARRIER_X, CARRIER_Y) < safe {
            return false;
        }
        if self
            .rocks
            .iter()
            .any(|o| js::dist(o.b.x, o.b.y, x, y) < o.b.r + r + 20.0)
        {
            return false;
        }
        !self
            .wrecks
            .iter()
            .any(|o| js::dist(o.b.x, o.b.y, x, y) < o.b.r * 1.4 + r + 120.0)
    }

    /// Seed the field: wrecks first (they need space), then rocks, then
    /// starter salvage near the carrier so the first hook comes quickly.
    pub fn populate(&mut self) {
        let (mut n, mut tries) = (0, 0);
        while n < START_WRECKS && tries < 400 {
            tries += 1;
            let x = self.rng.range(300.0, WORLD_W - 300.0);
            let y = self.rng.range(300.0, WORLD_H - 300.0);
            if self.clear_spot(x, y, 160.0, 1100.0) {
                let w = make_wreck(&mut self.rng, x, y, self.shift_no);
                self.wrecks.push(w);
                n += 1;
            }
        }
        let (mut n, mut tries) = (0, 0);
        while n < START_ROCKS && tries < 2000 {
            tries += 1;
            let roll = self.rng.next();
            let size = if roll < 0.4 {
                3
            } else if roll < 0.8 {
                2
            } else {
                1
            };
            let x = self.rng.range(100.0, WORLD_W - 100.0);
            let y = self.rng.range(100.0, WORLD_H - 100.0);
            if self.clear_spot(x, y, rock_size(size).0, 650.0) {
                let r = make_rock_drifting(&mut self.rng, size, x, y);
                self.rocks.push(r);
                n += 1;
            }
        }
        for _ in 0..START_ITEMS {
            let a = self.rng.angle();
            let d = self.rng.range(500.0, 1300.0);
            let kind = if self.rng.chance(0.6) {
                ItemType::Scrap
            } else {
                ItemType::Ore
            };
            let vx = self.rng.range(-10.0, 10.0);
            let vy = self.rng.range(-10.0, 10.0);
            let it = self.new_item(
                kind,
                CARRIER_X + a.cos() * d,
                CARRIER_Y + a.sin() * d,
                vx,
                vy,
            );
            self.items.push(it);
        }
    }

    /// A one-shot cable command. In the browser these run from key and
    /// button handlers between frames, i.e. before the next step.
    pub fn action(&mut self, a: Action) {
        if self.shift_over {
            return;
        }
        match a {
            Action::Hook => self.try_hook(),
            Action::Drop => self.drop_last(),
            Action::Cut => self.cut_all(),
        }
    }

    /// Advance one fixed step of `dt` seconds (normally [`STEP`]).
    pub fn step(&mut self, input: &Input, dt: f64) {
        if self.shift_over {
            return;
        }
        self.t += dt;
        self.time_left -= dt;

        self.update_ship(input, dt);
        // (Partner tug and magnetic scoop: milestones 5 and 3. Rivals: milestone 2.)
        for it in &mut self.items {
            it.angle += it.spin * dt;
            it.thrown_t = (it.thrown_t - dt).max(0.0);
            // (Reactor fuses: milestone 2, with volatile salvage from wrecks.)
        }
        for i in 0..self.rocks.len() {
            let r = &mut self.rocks[i];
            r.angle += r.spin * dt;
            r.flash = (r.flash - dt).max(0.0);
            // Temporary pebbles crumble to dust when their time is up.
            if let Some(c) = r.crumble_t.as_mut() {
                *c -= dt;
                if *c <= 0.0 && !r.dead {
                    r.dead = true;
                    let (x, y) = (r.b.x, r.b.y);
                    self.emit(Event::Burst {
                        x,
                        y,
                        n: 4,
                        tint: Tint::Rock,
                        speed: 30.0,
                    });
                }
            }
        }
        for w in &mut self.wrecks {
            w.angle += w.spin * dt;
            w.flash = (w.flash - dt).max(0.0);
        }

        self.integrate_all(dt);
        self.solve_cables(dt);
        self.resolve_collisions();
        // (Bullets, crabs, refinery, drones, collectors, law, miners, nets,
        // missiles and sentries update here in later milestones.)
        self.update_bay(dt);
        // (Tutorial, field events and spawns: milestones 5, 5 and 2.)
        self.shake = (self.shake - dt * 30.0).max(0.0);
        self.screen_flash = (self.screen_flash - dt * 1.6).max(0.0);
        self.sweep_dead();
        self.update_camera(dt);

        if !self.warned_minute && self.time_left <= 60.0 {
            self.warned_minute = true;
            self.radio(Radio::Minute, true, false);
        }
        if !self.warned_ten && self.time_left <= 10.0 {
            self.warned_ten = true;
            self.radio(Radio::TenSec, true, false);
        }
        if self.time_left <= 0.0 {
            self.shift_over = true;
        }
    }

    /// Tug flight: rotation and thrust, both blunted by the weight on the winch.
    fn update_ship(&mut self, input: &Input, dt: f64) {
        let s = &mut self.ship;
        s.flash = (s.flash - dt).max(0.0);
        if !s.alive {
            s.respawn_t -= dt;
            if s.respawn_t <= 0.0 {
                self.respawn_ship();
            }
            self.emit(Event::Thrust {
                level: 0.0,
                load: 0.0,
                sputtering: false,
            });
            return;
        }
        s.invuln = (s.invuln - dt).max(0.0);
        let load = self.tow_mass();
        let load_ratio = load / self.mods.winch_cap;
        let mods = &self.mods;
        let s = &mut self.ship;
        // (Gun and strafe heat: milestone 2.)

        // Max turn rate and responsiveness both drop with load, so a heavy
        // train makes the tug lazy to start and slow to stop rotating.
        let turn_load = load + mods.turn_load;
        let turn_max = SHIP_TURN * mods.turn_mul / (1.0 + turn_load / 7.0);
        let turn_resp = 9.0 / (1.0 + turn_load / 6.0);
        s.av += (input.turn * turn_max - s.av) * (turn_resp * dt).min(1.0);
        // (Net tearing: milestone 4.)
        s.angle += s.av * dt;
        s.spin_meter = s.spin_meter * (-0.35 * dt).exp() + (s.av.abs() / turn_max) * dt;

        // Forward wins over brake; reverse thrust is deliberately weaker.
        let throttle = if input.thrust > 0.0 {
            input.thrust
        } else {
            -REVERSE_THROTTLE * input.reverse
        };
        let mut force = SHIP_THRUST * mods.thrust_mul;
        // (Crab, net, stun and afterburner modifiers: milestones 2 and 4.)

        // Engines sputter under heavy loads: random short cut-outs.
        if throttle > 0.0 && load_ratio > SPUTTER_LOAD {
            s.sputter_t -= dt;
            if s.sputter_t <= 0.0 {
                s.sputter_t = self.rng.range(0.06, 0.4);
                s.sputtering = self.rng.chance(SPUTTER_CHANCE);
            }
            if s.sputtering {
                force *= SPUTTER_FORCE;
            }
        } else {
            s.sputtering = false;
        }

        s.b.vx += (s.angle.cos() * force * throttle / s.b.mass) * dt;
        s.b.vy += (s.angle.sin() * force * throttle / s.b.mass) * dt;
        s.thrust_level = throttle.abs();
        s.throttle = throttle;
        let (level, sputtering) = (s.thrust_level, s.sputtering);
        // (Side thrusters and firing: milestone 2.)

        let low = !s.low_warned && s.hp < 30.0;
        if low {
            s.low_warned = true;
        }

        // Shield recharge: waits out the delay after the last hit, then
        // refills slowly (faster while parked at the bay).
        s.shield_flash = (s.shield_flash - dt).max(0.0);
        s.shield_delay_t = (s.shield_delay_t - dt).max(0.0);
        let mut shield_up = false;
        if s.shield_delay_t <= 0.0 && s.shield < mods.shield_max {
            let in_bay = js::dist(s.b.x, s.b.y, BAY_X, BAY_Y) < BAY_REPAIR_RANGE;
            let boost = if in_bay { SHIELD_BAY_BOOST } else { 1.0 };
            s.shield = mods
                .shield_max
                .min(s.shield + mods.shield_rate * boost * dt);
            shield_up = s.shield >= mods.shield_max;
        }

        self.emit(Event::Thrust {
            level,
            load: load_ratio,
            sputtering,
        });
        if low {
            self.radio(Radio::HullLow, true, false);
        }
        if shield_up {
            self.sfx(Sfx::ShieldUp);
        }
    }

    /// All hull damage funnels through here, so the difficulty scale applies once.
    pub fn damage_ship(&mut self, n: f64) {
        let s = &mut self.ship;
        if !s.alive || s.invuln > 0.0 || n <= 0.0 {
            return;
        }
        let mut n = n * self.diff_damage;
        // The shield takes the hit first; any hit restarts the recharge delay.
        s.shield_delay_t = self.mods.shield_delay;
        if s.shield > 0.0 {
            let absorbed = s.shield.min(n);
            s.shield -= absorbed;
            n -= absorbed;
            s.shield_flash = 0.25;
            let down = s.shield <= 0.0;
            if down {
                s.shield = 0.0;
            }
            if absorbed > 1.0 {
                self.sfx(Sfx::ShieldHit);
            }
            if down {
                self.sfx(Sfx::ShieldDown);
                self.radio(Radio::ShieldDown, true, false);
            }
            if n <= 0.0 {
                self.shake = (self.shake + absorbed * 0.15).min(14.0);
                return;
            }
        }
        let s = &mut self.ship;
        s.hp -= n;
        s.flash = 0.15;
        let dead = s.hp <= 0.0;
        self.shake = (self.shake + n * 0.5).min(14.0);
        if n > 2.0 {
            self.sfx(Sfx::Hurt);
            self.emit(Event::Rumble {
                strength: (n / 25.0).min(1.0),
                ms: 160.0,
            });
        }
        if dead {
            self.kill_ship();
        }
    }

    /// Wreck the tug: cargo floats free, the deductible is charged, respawn soon.
    fn kill_ship(&mut self) {
        let s = &mut self.ship;
        s.alive = false;
        s.hp = 0.0;
        s.respawn_t = RESPAWN_TIME;
        let (x, y) = (s.b.x, s.b.y);
        self.stats.deaths += 1;
        // (The broker crew's fee multiplier joins with crews in milestone 3.)
        let deductible = js::round(DEDUCTIBLE * self.diff_fees);
        self.banked = (self.banked - deductible).max(0.0);
        self.popup(
            x,
            y - 30.0,
            format!("-{deductible} deductible"),
            Tint::Danger,
        );
        self.sever_at(0);
        // (Crabs, nets, the law and rogue sentries react here in later milestones.)
        self.explosion_fx(x, y, 150.0, Tint::Hazard);
        self.shake = 18.0;
        self.sfx(Sfx::Boom);
        self.radio(Radio::Dead, true, false);
    }

    fn respawn_ship(&mut self) {
        let s = &mut self.ship;
        s.b.x = BAY_X;
        s.b.y = BAY_Y;
        s.b.vx = 0.0;
        s.b.vy = 0.0;
        s.angle = std::f64::consts::PI;
        s.av = 0.0;
        s.hp = self.mods.max_hp;
        s.alive = true;
        s.invuln = SHIP_INVULN;
        s.low_warned = false;
        s.spin_meter = 0.0;
        s.shield = self.mods.shield_max;
        s.shield_delay_t = 0.0;
    }

    /// The fireball's lasting effects on the screen (explosionFx); the
    /// particles themselves are drawn by the game from the event.
    pub(crate) fn explosion_fx(&mut self, x: f64, y: f64, size: f64, tint: Tint) {
        let k = size / 100.0;
        let s = &self.ship;
        // The closer you are, the harder the flash and shake hit.
        let near = if s.alive {
            (1.0 - js::dist(x, y, s.b.x, s.b.y) / 1500.0).clamp(0.0, 1.0)
        } else {
            0.3
        };
        self.screen_flash = self
            .screen_flash
            .max((0.18 * k).min(0.5) * (0.35 + 0.65 * near));
        self.shake = (self.shake + size * 0.07 * (0.3 + 0.7 * near)).min(26.0);
        self.emit(Event::Explosion { x, y, size, tint });
    }

    /// The bay banks cargo dropped inside its ring, tractors nearby pieces
    /// in, and repairs a tug parked close by.
    fn update_bay(&mut self, dt: f64) {
        for i in 0..self.items.len() {
            let it = &self.items[i];
            // Charges and keycards have no resale; the bay won't take them.
            if it.dead || matches!(it.kind, ItemType::Charge | ItemType::Keycard) {
                continue;
            }
            let d = js::dist(it.b.x, it.b.y, BAY_X, BAY_Y);
            if d < BAY_R - it.b.r * 0.3 {
                self.bank_item(i);
                continue;
            }
            // Bay tractor: cargo trailing a cable-length behind a parked tug
            // would otherwise hang just outside the ring.
            let reach = BAY_R * BAY_TRACTOR_REACH;
            if d < reach {
                let pull = BAY_TRACTOR_PULL * (1.0 - d / reach) * dt;
                let it = &mut self.items[i];
                it.b.vx += ((BAY_X - it.b.x) / d) * pull;
                it.b.vy += ((BAY_Y - it.b.y) / d) * pull;
            }
        }
        let s = &mut self.ship;
        if s.alive && js::dist(s.b.x, s.b.y, BAY_X, BAY_Y) < BAY_REPAIR_RANGE {
            // (Sanctuary from the law: milestone 4.)
            let mut repaired_low = false;
            if s.hp < self.mods.max_hp {
                s.hp = self.mods.max_hp.min(s.hp + BAY_REPAIR_RATE * dt);
                repaired_low = s.hp < 70.0;
            }
            if s.hp > 40.0 {
                s.low_warned = false;
            }
            if repaired_low {
                self.radio(Radio::Repair, false, true);
            }
            // (Latched crabs die in the bay: milestone 2.)
        }
    }

    /// Credit an item. Bankings within two seconds of each other count as
    /// one "delivery", which the end screen reports as the best delivery.
    fn bank_item(&mut self, i: usize) {
        let it = &self.items[i];
        let (x, y, value, rare, hidden) = (it.b.x, it.b.y, it.value, it.rare, it.hidden);
        let tint = it.tint;
        let named = rare || hidden || it.kind == ItemType::Blackbox;
        let label = it.label;
        self.banked += value;
        self.stats.items += 1;
        if self.t - self.delivery_t < DELIVERY_WINDOW {
            self.delivery_total += value;
        } else {
            self.delivery_total = value;
        }
        self.delivery_t = self.t;
        self.stats.best_run = self.stats.best_run.max(self.delivery_total);
        // (Highlights and achievements: milestone 5.)
        let text = if named {
            format!("+{} {}", fmt_credits(value), label.to_uppercase())
        } else {
            format!("+{}", fmt_credits(value))
        };
        let rare_tint = if rare {
            tint.map(Tint::Rgb).unwrap_or(Tint::Text)
        } else {
            Tint::Hazard
        };
        self.popup(x, y - 20.0, text, rare_tint);
        self.emit(Event::Burst {
            x,
            y,
            n: if named { 26 } else { 14 },
            tint: if rare {
                tint.map(Tint::Rgb).unwrap_or(Tint::Hazard)
            } else {
                Tint::Hazard
            },
            speed: if named { 200.0 } else { 140.0 },
        });
        self.remove_item(i);
        self.sfx(if named { Sfx::Jackpot } else { Sfx::Bank });
        // (Cargo-pod reveal lines arrive with wrecks in milestone 2.)
        if rare || hidden {
            self.radio(Radio::Rare, true, false);
        } else if value >= 800.0 {
            self.radio(Radio::BankBig, true, false);
        } else {
            self.radio(Radio::Bank, false, false);
        }
    }

    /// Remove an item from the field, unhooking it from the chain.
    pub fn remove_item(&mut self, i: usize) {
        if self.items[i].dead {
            return;
        }
        if self.items[i].towed {
            let id = self.items[i].id;
            self.remove_from_chain(id);
        }
        self.items[i].dead = true;
    }

    fn sweep_dead(&mut self) {
        self.rocks.retain(|r| !r.dead);
        self.items.retain(|i| !i.dead);
        self.wrecks.retain(|w| !w.dead);
        let items = &self.items;
        self.chain.retain(|id| items.iter().any(|it| it.id == *id));
    }

    /// The camera leads the tug's velocity and zooms out as the cargo train
    /// grows, so a long swinging load stays on screen.
    fn update_camera(&mut self, dt: f64) {
        let s = &self.ship;
        let base = (self.view.0.min(self.view.1) / 900.0).clamp(0.5, 1.1);
        let span = self
            .chain
            .iter()
            .filter_map(|id| self.items.iter().find(|it| it.id == *id))
            .map(|it| it.b.dist_to(&s.b))
            .fold(0.0, f64::max);
        let target_zoom = base * (1.0 - (span / 2200.0).min(0.32));
        let (tx, ty) = if s.alive {
            (s.b.x + s.b.vx * 0.35, s.b.y + s.b.vy * 0.35)
        } else {
            (self.cam.x, self.cam.y)
        };
        // (Co-op framing of both tugs: milestone 5.)
        let k = 1.0 - (-4.0 * dt).exp();
        self.cam.x += (tx - self.cam.x) * k;
        self.cam.y += (ty - self.cam.y) * k;
        self.cam.zoom += (target_zoom - self.cam.zoom) * (1.0 - (-2.0 * dt).exp());
    }

    /// Total mass on the winch, tonnes.
    pub fn tow_mass(&self) -> f64 {
        self.chain
            .iter()
            .filter_map(|id| self.items.iter().find(|it| it.id == *id))
            .map(|it| it.b.mass)
            // Not .sum(): an empty f64 sum is -0.0, which shows as "-0 t".
            .fold(0.0, |a, b| a + b)
    }

    /// Total value of the cargo on the line.
    pub fn line_value(&self) -> f64 {
        self.chain
            .iter()
            .filter_map(|id| self.items.iter().find(|it| it.id == *id))
            .map(|it| it.value)
            .fold(0.0, |a, b| a + b)
    }
}

/// Whole credits with thousands separators, like the browser's fmt().
pub fn fmt_credits(n: f64) -> String {
    let v = js::round(n) as i64;
    let digits = v.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if v < 0 {
        format!("-{out}")
    } else {
        out
    }
}
