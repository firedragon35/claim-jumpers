//! Rival claim jumpers and field refills (chooseJumperTarget, steer,
//! updateJumper and updateSpawns in src/js/16-simulation-step.js;
//! scatterRivals and callCover in 10-field-events.js).

use crate::consts::*;
use crate::entities::{make_rock, make_wreck, Body, Bullet, JumperState};
use crate::events::{Radio, Sfx};
use crate::js;
use crate::world::World;

/// Accelerate toward a desired velocity aimed at (tx, ty), with the
/// change capped per step, and face the target.
fn steer(b: &mut Body, angle: &mut f64, tx: f64, ty: f64, max_speed: f64, accel: f64, dt: f64) {
    let (dx, dy) = (tx - b.x, ty - b.y);
    let d = nonzero(js::hypot(dx, dy));
    let dvx = (dx / d) * max_speed - b.vx;
    let dvy = (dy / d) * max_speed - b.vy;
    let dv = nonzero(js::hypot(dvx, dvy));
    let k = ((accel * dt) / dv).min(1.0);
    b.vx += dvx * k;
    b.vy += dvy * k;
    *angle = dy.atan2(dx);
}

/// The browser's `x || 1`: a zero length divides as 1.
fn nonzero(x: f64) -> f64 {
    if x == 0.0 {
        1.0
    } else {
        x
    }
}

impl World {
    /// What a rival should steal: the richest piece nearby, counting your
    /// towed cargo double (stealing from you is the business).
    fn choose_jumper_target(&self, j: usize) -> Option<u32> {
        let jm = &self.jumpers[j];
        let mut best = None;
        let mut best_score = f64::NEG_INFINITY;
        for it in &self.items {
            if it.dead || it.value == 0.0 || it.hooked_by.is_some_and(|h| h != jm.id) {
                continue;
            }
            let d = it.b.dist_to(&jm.b);
            if d > 2400.0 {
                continue;
            }
            let score = it.value * if it.towed { 2.0 } else { 1.0 } - d * 0.4;
            if score > best_score {
                best = Some(it.id);
                best_score = score;
            }
        }
        best
    }

    /// Fire a volley at the tug if it is in range and the gun is ready,
    /// leading the shot toward where the tug is heading. (Escorted haulers
    /// as an alternative target: milestone 5.)
    fn jumper_fire_at_ship(&mut self, j: usize) {
        let s = &self.ship;
        if !s.alive {
            return;
        }
        let tgt = s.b;
        let jm = &self.jumpers[j];
        let d = jm.b.dist_to(&tgt);
        if jm.fire_t > 0.0 || d > JUMPER_FIRE_RANGE {
            return;
        }
        let v = jm.variant.def();
        let st = &RIVAL_STAGES[self.rival_stage()];
        let fire_t = self.rng.range(st.fire.0, st.fire.1) * v.fire;
        let lead = d / EBULLET_SPEED;
        let jb = self.jumpers[j].b;
        let a = (tgt.y + tgt.vy * lead - jb.y).atan2(tgt.x + tgt.vx * lead - jb.x)
            + self.rng.range(-0.12, 0.12);
        self.jumpers[j].fire_t = fire_t;
        // Gunships fan their volley slightly so one dodge does not clear both bolts.
        for k in 0..v.shots {
            let ak = a + (f64::from(k) - f64::from(v.shots - 1) / 2.0) * 0.12;
            self.ebullets.push(Bullet {
                x: jb.x,
                y: jb.y,
                vx: ak.cos() * EBULLET_SPEED,
                vy: ak.sin() * EBULLET_SPEED,
                life: EBULLET_LIFE,
                dmg: EBULLET_DAMAGE,
            });
        }
        self.sfx(Sfx::EnemyShot(d.min(f64::from(u16::MAX)) as u16));
        self.under_attack();
    }

    pub(crate) fn update_jumper(&mut self, j: usize, dt: f64) {
        {
            let jm = &mut self.jumpers[j];
            jm.flash = (jm.flash - dt).max(0.0);
            jm.fire_t -= dt;
            jm.retarget -= dt;
        }
        // Its hooked piece is gone (banked, blown up): back to hunting.
        if let Some(id) = self.jumpers[j].hook {
            if self.item_index(id).is_none_or(|i| self.items[i].dead) {
                self.jumpers[j].hook = None;
                self.jumpers[j].state = JumperState::Hunt;
            }
        }
        let stage = self.rival_stage();
        let sp = self.jumpers[j].speed_mul;
        let s = self.ship.b;
        let alive = self.ship.alive;

        // Scattering after a wingmate got smashed: run from the tug, hold fire.
        if self.jumpers[j].flee_t > 0.0 {
            let jm = &mut self.jumpers[j];
            jm.flee_t -= dt;
            if alive {
                let a = (jm.b.y - s.y).atan2(jm.b.x - s.x);
                let (tx, ty) = (jm.b.x + a.cos() * 300.0, jm.b.y + a.sin() * 300.0);
                steer(&mut jm.b, &mut jm.angle, tx, ty, 400.0 * sp, 500.0, dt);
            }
            return;
        }

        // Hunters ignore the cargo: circle the tug tightly and keep firing.
        // A jumper giving covering fire to a latched wingmate does the same.
        {
            let jm = &mut self.jumpers[j];
            if jm.cover_t > 0.0 && jm.hook.is_none() {
                jm.cover_t -= dt;
            }
        }
        let jm = &self.jumpers[j];
        if jm.hunter || (jm.cover_t > 0.0 && jm.hook.is_none()) {
            if alive {
                let jm = &mut self.jumpers[j];
                jm.orbit += dt * 1.1;
                let (tx, ty) = (
                    s.x + jm.orbit.cos() * HUNTER_ORBIT,
                    s.y + jm.orbit.sin() * HUNTER_ORBIT,
                );
                steer(&mut jm.b, &mut jm.angle, tx, ty, 380.0 * sp, 420.0, dt);
                jm.angle = (s.y - jm.b.y).atan2(s.x - jm.b.x);
            }
            self.jumper_fire_at_ship(j);
            return;
        }

        if self.jumpers[j].state == JumperState::Hunt {
            let jm = &self.jumpers[j];
            let stale = match jm.target.and_then(|id| self.item_index(id)) {
                None => true,
                Some(i) => {
                    let t = &self.items[i];
                    t.dead || t.hooked_by.is_some_and(|h| h != jm.id)
                }
            };
            if jm.retarget <= 0.0 || stale {
                let t = self.choose_jumper_target(j);
                let jm = &mut self.jumpers[j];
                jm.target = t;
                jm.retarget = 1.5;
            }
            if let Some(ti) = self.jumpers[j].target.and_then(|id| self.item_index(id)) {
                let tb = self.items[ti].b;
                let jm = &mut self.jumpers[j];
                // "Arrive" steering: ease off near the target. Ramming a
                // heavy piece at full speed would count as a cargo smash.
                let approach = (340.0 * sp).min(60.0 + (jm.b.dist_to(&tb) - tb.r) * 0.8);
                steer(&mut jm.b, &mut jm.angle, tb.x, tb.y, approach, 300.0, dt);
                if jm.b.dist_to(&tb) < jm.b.r + tb.r + 34.0 {
                    jm.state = JumperState::Haul;
                    jm.hook = jm.target;
                    jm.link_len = jm.b.dist_to(&tb) + 6.0;
                    jm.latch_t = 0.0;
                    let jid = jm.id;
                    self.items[ti].hooked_by = Some(jid);
                    if self.items[ti].towed {
                        self.radio(Radio::Latched, true, false);
                        self.call_cover(j);
                    }
                }
            } else if alive {
                // Nothing worth stealing: circle the tug and take pot shots.
                let jm = &mut self.jumpers[j];
                jm.orbit += dt * 0.6;
                let (tx, ty) = (s.x + jm.orbit.cos() * 340.0, s.y + jm.orbit.sin() * 340.0);
                steer(&mut jm.b, &mut jm.angle, tx, ty, 300.0 * sp, 260.0, dt);
            }
            self.jumper_fire_at_ship(j);
            return;
        }

        // From Annoyed on, they keep shooting at you while they drag your
        // cargo off.
        if RIVAL_STAGES[stage].haul_fire {
            self.jumper_fire_at_ship(j);
        }

        // Hauling: pull away from the tug and out toward the field edge.
        let Some(ii) = self.jumpers[j].hook.and_then(|id| self.item_index(id)) else {
            self.jumpers[j].state = JumperState::Hunt;
            return;
        };
        let (fx, fy) = if alive {
            (s.x, s.y)
        } else {
            (CARRIER_X, CARRIER_Y)
        };
        let jm = &mut self.jumpers[j];
        let (mut ax, mut ay) = (jm.b.x - fx, jm.b.y - fy);
        let al = nonzero(js::hypot(ax, ay));
        let (ox, oy) = (jm.b.x - CARRIER_X, jm.b.y - CARRIER_Y);
        let ol = nonzero(js::hypot(ox, oy));
        ax = (ax / al) * 0.6 + (ox / ol) * 0.4;
        ay = (ay / al) * 0.6 + (oy / ol) * 0.4;
        let l = nonzero(js::hypot(ax, ay));
        let haul = JUMPER_HAUL_ACCEL * jm.variant.def().haul;
        jm.b.vx += (ax / l) * haul * dt;
        jm.b.vy += (ay / l) * haul * dt;
        jm.angle = ay.atan2(ax);

        // Latched onto your train long enough, they cut your line at that piece.
        let item_id = self.items[ii].id;
        if self.items[ii].towed {
            let jm = &mut self.jumpers[j];
            jm.latch_t += dt;
            if jm.latch_t > JUMPER_SEVER_TIME * jm.variant.def().sever {
                jm.latch_t = 0.0;
                if let Some(idx) = self.chain.iter().position(|c| *c == item_id) {
                    self.sever_at(idx);
                }
                self.radio(Radio::Severed, true, false);
            }
        } else {
            self.jumpers[j].latch_t = 0.0;
        }

        // Out of the field with your salvage: it is gone.
        let b = self.jumpers[j].b;
        let m = JUMPER_ESCAPE_MARGIN;
        if b.x < -m || b.y < -m || b.x > WORLD_W + m || b.y > WORLD_H + m {
            self.stats.lost += self.items[ii].value;
            self.remove_item(ii);
            self.jumpers[j].dead = true;
            self.radio(Radio::Escaped, true, false);
        }
    }

    /// Rivals near a wingmate smashed by cargo break off and flee.
    pub(crate) fn scatter_rivals(&mut self, dead: usize) {
        let db = self.jumpers[dead].b;
        let mut fled = 0;
        for o in 0..self.jumpers.len() {
            // (Boss crews never scatter: milestone 5.)
            if o == dead || self.jumpers[o].dead || self.jumpers[o].b.dist_to(&db) > SCATTER_RANGE {
                continue;
            }
            self.jumpers[o].flee_t = SCATTER_TIME;
            if let Some(id) = self.jumpers[o].hook.take() {
                if let Some(i) = self.item_index(id) {
                    self.items[i].hooked_by = None;
                }
                self.jumpers[o].state = JumperState::Hunt;
            }
            fled += 1;
        }
        if fled > 0 {
            self.radio(Radio::Scatter, false, false);
        }
    }

    /// From Hostile on, a crew that gets a line on your cargo calls in
    /// covering fire from wingmates nearby.
    fn call_cover(&mut self, j: usize) {
        if self.rival_stage() < 2 {
            return;
        }
        let jb = self.jumpers[j].b;
        for o in 0..self.jumpers.len() {
            let om = &mut self.jumpers[o];
            if o != j && !om.dead && om.hook.is_none() && om.b.dist_to(&jb) < COVER_RANGE {
                om.cover_t = COVER_TIME;
            }
        }
    }

    /// Rival arrivals and field refills. A loaded cable draws jumpers
    /// sooner, up to twice as fast. (Collectors, the law, the refinery and
    /// the mining crew arrive here in milestone 4; the tutorial holds
    /// rivals back in milestone 5.)
    pub(crate) fn update_spawns(&mut self, dt: f64) {
        self.jumper_timer -= dt * (1.0 + self.line_value() / 3000.0).min(2.0);
        if self.jumper_timer <= 0.0 {
            // The grudge stage raises how many can be out at once and how
            // often they come; at Vendetta they arrive in pairs.
            let st = &RIVAL_STAGES[self.rival_stage()];
            let cap =
                (jumper_cap_for(self.shift_no) + self.diff_jumper_cap + st.cap).clamp(1.0, 7.0);
            let count = if st.pack { 2 } else { 1 };
            let rate = st.rate;
            let (px, py) = self.edge_point(1200.0);
            let mut spawned = 0;
            for i in 0..count {
                if self.jumpers.len() as f64 >= cap {
                    break;
                }
                let k = f64::from(i) * 40.0;
                let jm = self.new_jumper(px + k, py + k, None);
                self.jumpers.push(jm);
                spawned += 1;
            }
            if spawned > 0 {
                let n = self.jumpers.len();
                let hunter = self.jumpers[n - spawned..].iter().any(|j| j.hunter);
                self.radio(
                    if hunter { Radio::Hunter } else { Radio::Jumper },
                    false,
                    false,
                );
            }
            self.jumper_timer = jumper_interval_for(self.shift_no, self.t)
                / (self.diff_jumper_rate * rate * self.belt_rivals)
                * self.rng.range(0.8, 1.2);
        }

        self.rock_timer -= dt;
        if self.rock_timer <= 0.0 {
            self.rock_timer = ROCK_REFILL_TIME;
            if self.rocks.len() < ROCK_REFILL_CAP {
                let (px, py) = self.edge_point(900.0);
                let a = (CARRIER_Y - py).atan2(CARRIER_X - px) + self.rng.range(-0.6, 0.6);
                let sp = self.rng.range(20.0, 50.0);
                let veined = self.rng.chance(0.12);
                let r = make_rock(
                    &mut self.rng,
                    3,
                    px.clamp(70.0, WORLD_W - 70.0),
                    py.clamp(70.0, WORLD_H - 70.0),
                    a.cos() * sp,
                    a.sin() * sp,
                    veined,
                );
                self.rocks.push(r);
            }
        }

        self.wreck_timer -= dt;
        if self.wreck_timer <= 0.0 {
            self.wreck_timer = WRECK_REFILL_TIME;
            if self.wrecks.len() < WRECK_REFILL_CAP {
                let (px, py) = self.edge_point(1200.0);
                let mut w = make_wreck(
                    &mut self.rng,
                    px.clamp(200.0, WORLD_W - 200.0),
                    py.clamp(200.0, WORLD_H - 200.0),
                    self.shift_no,
                );
                let a = (CARRIER_Y - w.b.y).atan2(CARRIER_X - w.b.x);
                w.b.vx = a.cos() * 25.0;
                w.b.vy = a.sin() * 25.0;
                self.wrecks.push(w);
                self.radio(Radio::NewWreck, false, false);
            }
        }
    }
}
