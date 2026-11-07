//! Scrap crabs (killCrab, flingCrab and updateCrab in
//! src/js/16-simulation-step.js).
//!
//! They hide in wrecks and ride out on the salvage. Once their piece is
//! towed they walk the cable toward the tug, hopping link to link, and
//! latch onto the hull to chew it. A hard yank throws a crawling crab off;
//! spinning the tug throws a latched one off; the bay pries them off.

use crate::consts::*;
use crate::entities::{Crab, CrabState};
use crate::events::{Radio, Sfx, Tint};
use crate::js;
use crate::world::World;
use std::f64::consts::{PI, TAU};

fn crab_dist(c: &Crab, x: f64, y: f64) -> f64 {
    js::dist(c.x, c.y, x, y)
}

impl World {
    pub fn kill_crab(&mut self, c: usize) {
        let cr = &mut self.crabs[c];
        if cr.dead {
            return;
        }
        cr.dead = true;
        let (x, y) = (cr.x, cr.y);
        self.burst(x, y, 10, Tint::Crab, 140.0);
        self.sfx(Sfx::Chip);
    }

    /// Knock a crab loose with a velocity; it is dazed for a moment.
    pub(crate) fn fling_crab(&mut self, c: usize, vx: f64, vy: f64) {
        let cr = &mut self.crabs[c];
        cr.state = CrabState::Free;
        cr.host = None;
        cr.vx = vx;
        cr.vy = vy;
        cr.stun_t = 1.4;
    }

    pub(crate) fn update_crab(&mut self, c: usize, dt: f64) {
        if self.crabs[c].dead {
            return;
        }
        self.crabs[c].leg_t += dt * 12.0;
        let alive = self.ship.alive;
        match self.crabs[c].state {
            CrabState::Ride => {
                let host = self.crabs[c].host.and_then(|id| self.item_index(id));
                let Some(h) = host.filter(|&h| !self.items[h].dead) else {
                    let cr = &mut self.crabs[c];
                    cr.state = CrabState::Free;
                    cr.host = None;
                    return;
                };
                let hi = &self.items[h];
                let (hx, hy, ha, hr, towed) = (hi.b.x, hi.b.y, hi.angle, hi.b.r, hi.towed);
                let cr = &mut self.crabs[c];
                cr.x = hx + (ha + cr.off_a).cos() * hr * 0.75;
                cr.y = hy + (ha + cr.off_a).sin() * hr * 0.75;
                if towed && alive {
                    cr.ride_t += dt;
                    if cr.ride_t > 0.8 {
                        cr.state = CrabState::Crawl;
                        cr.t = 0.0;
                        self.radio(Radio::Crab, true, true);
                    }
                }
            }
            CrabState::Crawl => {
                let host_id = self.crabs[c].host;
                let idx = host_id.and_then(|id| self.chain.iter().position(|x| *x == id));
                let Some(idx) = idx.filter(|_| alive) else {
                    let cr = &mut self.crabs[c];
                    cr.state = CrabState::Ride;
                    cr.ride_t = 0.0;
                    return;
                };
                let anchor = self.anchor_body(idx);
                let h = self.item_index(self.chain[idx]).expect("chain item exists");
                let hb = self.items[h].b;
                let jerk = self.items[h].jerk;
                let seg_len = js::dist(hb.x, hb.y, anchor.x, anchor.y).max(40.0);
                let cr = &mut self.crabs[c];
                cr.t += (CRAB_CRAWL_SPEED * dt) / seg_len;
                cr.x = hb.x + (anchor.x - hb.x) * cr.t;
                cr.y = hb.y + (anchor.y - hb.y) * cr.t;
                // A hard yank on this link throws the crab off sideways.
                if jerk > JERK_SHAKE {
                    let side = if self.rng.next() < 0.5 { 1.0 } else { -1.0 };
                    let a = (anchor.y - hb.y).atan2(anchor.x - hb.x) + PI / 2.0 * side;
                    self.fling_crab(c, hb.vx + a.cos() * 220.0, hb.vy + a.sin() * 220.0);
                    let (x, y) = (self.crabs[c].x, self.crabs[c].y);
                    self.popup(x, y - 14.0, "SHOOK".to_owned(), Tint::Cable);
                    return;
                }
                if self.crabs[c].t >= 1.0 {
                    if idx == 0 {
                        let off_a = self.rng.range(0.0, TAU);
                        let cr = &mut self.crabs[c];
                        cr.state = CrabState::Latched;
                        cr.off_a = off_a;
                        self.radio(Radio::CrabHull, true, false);
                    } else {
                        let cr = &mut self.crabs[c];
                        cr.host = Some(self.chain[idx - 1]);
                        cr.t = 0.0;
                    }
                }
            }
            CrabState::Latched => {
                if !alive {
                    let vx = self.rng.range(-100.0, 100.0);
                    let vy = self.rng.range(-100.0, 100.0);
                    self.fling_crab(c, vx, vy);
                    return;
                }
                let s = &mut self.ship;
                let cr = &mut self.crabs[c];
                cr.x = s.b.x + (s.angle + cr.off_a).cos() * (s.b.r - 2.0);
                cr.y = s.b.y + (s.angle + cr.off_a).sin() * (s.b.r - 2.0);
                // Straight off the hull, under the shield, without the
                // per-hit effects damage_ship would raise every step.
                if s.invuln <= 0.0 {
                    s.hp -= CRAB_HULL_DPS * self.diff_damage * dt;
                }
                // Audible chewing, a few chomps a second.
                cr.chomp_t -= dt;
                let chomp = cr.chomp_t <= 0.0;
                if chomp {
                    cr.chomp_t = 0.35;
                }
                let dead = s.hp <= 0.0;
                if chomp {
                    self.sfx(Sfx::Chomp);
                }
                if dead {
                    self.kill_ship();
                }
                let s = &self.ship;
                if s.spin_meter > CRAB_SPIN_TO_SHAKE {
                    let a = s.angle + self.crabs[c].off_a;
                    let (vx, vy) = (s.b.vx + a.cos() * 260.0, s.b.vy + a.sin() * 260.0);
                    self.fling_crab(c, vx, vy);
                    self.radio(Radio::CrabOff, false, false);
                }
            }
            CrabState::Free => {
                let cr = &mut self.crabs[c];
                cr.stun_t = (cr.stun_t - dt).max(0.0);
                cr.x += cr.vx * dt;
                cr.y += cr.vy * dt;
                let k = (-0.6 * dt).exp();
                cr.vx *= k;
                cr.vy *= k;
                if cr.stun_t > 0.0 {
                    return;
                }
                let s = self.ship.b;
                let sa = self.ship.angle;
                if alive && crab_dist(cr, s.x, s.y) < 450.0 {
                    let a = (s.y - cr.y).atan2(s.x - cr.x);
                    cr.vx += a.cos() * 90.0 * dt;
                    cr.vy += a.sin() * 90.0 * dt;
                    if crab_dist(cr, s.x, s.y) < s.r + cr.r {
                        cr.state = CrabState::Latched;
                        cr.off_a = (cr.y - s.y).atan2(cr.x - s.x) - sa;
                        self.radio(Radio::CrabHull, true, false);
                        return;
                    }
                }
                // Swung cargo crushes loose crabs; slow cargo gets climbed onto.
                for i in 0..self.items.len() {
                    let it = &self.items[i];
                    let cr = &self.crabs[c];
                    if it.dead || crab_dist(cr, it.b.x, it.b.y) > it.b.r + cr.r {
                        continue;
                    }
                    if js::hypot(it.b.vx - cr.vx, it.b.vy - cr.vy) > 160.0 {
                        self.kill_crab(c);
                        break;
                    }
                    let id = it.id;
                    let off_a = self.rng.range(0.0, TAU);
                    let cr = &mut self.crabs[c];
                    cr.state = CrabState::Ride;
                    cr.host = Some(id);
                    cr.ride_t = 0.0;
                    cr.off_a = off_a;
                    break;
                }
            }
        }
    }
}
