//! Integration, circle collisions and impact damage (integrateAll,
//! collide, onImpact and resolveCollisions in src/js/16-simulation-step.js).

use crate::consts::*;
use crate::entities::{make_rock, Body, JumperState};
use crate::events::{Event, Sfx, Tint};
use crate::items::ItemType;
use crate::opposition::{CollectorState, SheriffState};
use crate::world::World;
use std::f64::consts::TAU;

/// Which entity a collision body belongs to.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Handle {
    /// A pilot's tug, by index into `World::pilots`.
    Ship(usize),
    Rock(usize),
    Item(usize),
    Wreck(usize),
    Jumper(usize),
    Drone(usize),
    Collector(usize),
    Miner(usize),
    Sheriff(usize),
    /// The refinery's solid hub (static).
    Refinery,
    /// One of the carrier's static hull circles (index into CARRIER_HULL).
    Hull(usize),
}

/// Carrier hull circle `k` as a static body.
fn hull_body(k: usize) -> Body {
    let (dx, r) = CARRIER_HULL[k];
    Body {
        x: CARRIER_X + dx,
        y: CARRIER_Y,
        r,
        mass: f64::INFINITY,
        fixed: true,
        ..Body::default()
    }
}

/// Move a body, damp it, and (if bounded) push it back from the field edge
/// with a soft spring rather than a hard bounce.
fn integrate(b: &mut Body, dt: f64, damp: f64, bounded: bool, (lo, hi): (f64, f64)) {
    b.x += b.vx * dt;
    b.y += b.vy * dt;
    if damp != 0.0 {
        let k = (-damp * dt).exp();
        b.vx *= k;
        b.vy *= k;
    }
    if !bounded {
        return;
    }
    let (m, push) = (WALL_MARGIN, WALL_PUSH);
    if b.x < lo + m {
        b.vx += (lo + m - b.x) * push * dt;
    }
    if b.x > hi - m {
        b.vx -= (b.x - (hi - m)) * push * dt;
    }
    if b.y < lo + m {
        b.vy += (lo + m - b.y) * push * dt;
    }
    if b.y > hi - m {
        b.vy -= (b.y - (hi - m)) * push * dt;
    }
}

/// Circle against circle: positional separation by inverse mass, then a
/// restitution impulse. Returns the closing speed if they struck.
fn collide(a: &mut Body, b: &mut Body) -> Option<f64> {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let rr = a.r + b.r;
    if dx.abs() > rr || dy.abs() > rr {
        return None;
    }
    let d2 = dx * dx + dy * dy;
    if d2 >= rr * rr {
        return None;
    }
    // `|| 0.001` in the browser: coincident centres separate along nothing.
    // A literal float pattern compares with `==`, so this matches exactly when the guard did.
    let d = match d2.sqrt() {
        0.0 => 0.001,
        v => v,
    };
    let (nx, ny) = (dx / d, dy / d);
    let (wa, wb) = (a.inv_mass(), b.inv_mass());
    let ws = wa + wb;
    if ws == 0.0 {
        return None;
    }
    let pen = rr - d;
    a.x -= nx * pen * (wa / ws);
    a.y -= ny * pen * (wa / ws);
    b.x += nx * pen * (wb / ws);
    b.y += ny * pen * (wb / ws);
    let rv = (b.vx - a.vx) * nx + (b.vy - a.vy) * ny;
    if rv >= 0.0 {
        return None;
    }
    let j = (-(1.0 + RESTITUTION) * rv) / ws;
    a.vx -= nx * j * wa;
    a.vy -= ny * j * wa;
    b.vx += nx * j * wb;
    b.vy += ny * j * wb;
    Some(-rv)
}

impl World {
    /// Moves the tug, rocks and other bodies one step with their damping.
    pub(crate) fn integrate_all(&mut self, dt: f64) {
        let edges = (self.edge_lo, self.edge_hi);
        for pl in &mut self.pilots {
            if pl.ship.alive {
                // A netted tug has no wall: the collector can drag it out.
                let bounded = pl.ship.net.is_none();
                integrate(&mut pl.ship.b, dt, SHIP_DAMP, bounded, edges);
            }
        }
        for r in &mut self.rocks {
            // Storm debris settles over a few seconds; normal drift is undamped.
            let damp = if r.b.speed() > STORM_SPEED {
                STORM_DAMP
            } else {
                0.0
            };
            integrate(&mut r.b, dt, damp, true, edges);
        }
        // A rival-hauled item may be dragged out of the field.
        for it in &mut self.items {
            let bounded = it.hooked_by.is_none();
            integrate(&mut it.b, dt, ITEM_DAMP, bounded, edges);
        }
        for w in &mut self.wrecks {
            integrate(&mut w.b, dt, WRECK_DAMP, true, edges);
        }
        for j in &mut self.jumpers {
            let bounded = j.state != JumperState::Haul;
            integrate(&mut j.b, dt, JUMPER_DAMP, bounded, edges);
        }
        for d in &mut self.drones {
            integrate(&mut d.b, dt, 0.5, true, edges);
        }
        for c in &mut self.collectors {
            let bounded = !matches!(c.state, CollectorState::Tow | CollectorState::Leave);
            integrate(&mut c.b, dt, 0.4, bounded, edges);
        }
        for m in &mut self.miners {
            integrate(&mut m.b, dt, 0.5, true, edges);
        }
        for s in &mut self.sheriffs {
            let bounded = s.state != SheriffState::Leave;
            integrate(&mut s.b, dt, 0.4, bounded, edges);
        }
    }

    fn body(&self, h: Handle) -> Body {
        match h {
            Handle::Ship(p) => self.pilots[p].ship.b,
            Handle::Rock(i) => self.rocks[i].b,
            Handle::Item(i) => self.items[i].b,
            Handle::Wreck(i) => self.wrecks[i].b,
            Handle::Jumper(i) => self.jumpers[i].b,
            Handle::Drone(i) => self.drones[i].b,
            Handle::Collector(i) => self.collectors[i].b,
            Handle::Miner(i) => self.miners[i].b,
            Handle::Sheriff(i) => self.sheriffs[i].b,
            Handle::Refinery => self.refinery.as_ref().expect("refinery").b,
            Handle::Hull(k) => hull_body(k),
        }
    }

    fn set_body(&mut self, h: Handle, b: Body) {
        match h {
            Handle::Ship(p) => self.pilots[p].ship.b = b,
            Handle::Rock(i) => self.rocks[i].b = b,
            Handle::Item(i) => self.items[i].b = b,
            Handle::Wreck(i) => self.wrecks[i].b = b,
            Handle::Jumper(i) => self.jumpers[i].b = b,
            Handle::Drone(i) => self.drones[i].b = b,
            Handle::Collector(i) => self.collectors[i].b = b,
            Handle::Miner(i) => self.miners[i].b = b,
            Handle::Sheriff(i) => self.sheriffs[i].b = b,
            Handle::Refinery | Handle::Hull(_) => {}
        }
    }

    fn is_dead(&self, h: Handle) -> bool {
        match h {
            Handle::Rock(i) => self.rocks[i].dead,
            Handle::Item(i) => self.items[i].dead,
            Handle::Wreck(i) => self.wrecks[i].dead,
            Handle::Jumper(i) => self.jumpers[i].dead,
            Handle::Drone(i) => self.drones[i].dead,
            Handle::Collector(i) => self.collectors[i].dead,
            Handle::Miner(i) => self.miners[i].dead,
            Handle::Sheriff(i) => self.sheriffs[i].dead,
            Handle::Ship(_) | Handle::Hull(_) | Handle::Refinery => false,
        }
    }

    /// Cargo on a cable flies over the carrier instead of colliding with it:
    /// a rope pulling a piece into a hull circle, with the tug on the other
    /// side, would pin it there for good. Loose cargo still bumps off.
    fn passes_carrier(&self, hull: Handle, it: Handle) -> bool {
        match (hull, it) {
            (Handle::Hull(_), Handle::Item(i)) => {
                self.items[i].towed || self.items[i].hooked_by.is_some()
            }
            _ => false,
        }
    }

    /// Every pair of bodies, in the browser's order: tug, rocks, items,
    /// wrecks, jumpers, then the carrier hull. Bodies work on a copy so a pair can
    /// be borrowed mutably; each impact syncs the pair back to the world
    /// first, because damage rules read positions (a broken rock spawns
    /// its pieces where it now is). Bodies only spawn, never vanish,
    /// during the pass, so the copies' indices stay valid.
    pub(crate) fn resolve_collisions(&mut self) {
        self.resolve_collisions_for(None);
    }

    /// Advances only pilot `p`'s tug and the cargo on its cable one step,
    /// for a client predicting its own tug between snapshots (CJN-16): the
    /// tug's own flight, its cargo's motion, its cable and its contacts.
    /// Contacts push but deal no damage, and nothing else in the field is
    /// simulated; the server stays the authority and the next snapshot
    /// corrects whatever the guess got wrong.
    pub fn predict_step(&mut self, p: usize, input: &crate::input::Input, dt: f64) {
        self.with_hearer(Some(p), |w| w.update_ship(p, input, dt));
        let edges = (self.edge_lo, self.edge_hi);
        let chain = self.chain_indices(p);
        let s = &mut self.pilots[p].ship;
        if s.alive {
            let bounded = s.net.is_none();
            integrate(&mut s.b, dt, SHIP_DAMP, bounded, edges);
        }
        for i in chain {
            let it = &mut self.items[i];
            let bounded = it.hooked_by.is_none();
            integrate(&mut it.b, dt, ITEM_DAMP, bounded, edges);
        }
        self.solve_cables_for(Some(p), dt);
        self.resolve_collisions_for(Some(p));
    }

    /// The collision pass; with `only` set, just the pairs involving that
    /// pilot's tug or cargo, pushed apart without damage (prediction).
    fn resolve_collisions_for(&mut self, only: Option<usize>) {
        let chain: Vec<usize> = only.map_or_else(Vec::new, |p| self.chain_indices(p));
        let predicted = |h: Handle| match (only, h) {
            (Some(p), Handle::Ship(q)) => p == q,
            (Some(_), Handle::Item(i)) => chain.contains(&i),
            _ => false,
        };
        let mut bodies: Vec<(Handle, Body)> = Vec::new();
        for (p, pl) in self.pilots.iter().enumerate() {
            if pl.ship.alive {
                bodies.push((Handle::Ship(p), pl.ship.b));
            }
        }
        bodies.extend(
            self.rocks
                .iter()
                .enumerate()
                .map(|(i, r)| (Handle::Rock(i), r.b)),
        );
        bodies.extend(
            self.items
                .iter()
                .enumerate()
                .map(|(i, it)| (Handle::Item(i), it.b)),
        );
        bodies.extend(
            self.wrecks
                .iter()
                .enumerate()
                .map(|(i, w)| (Handle::Wreck(i), w.b)),
        );
        bodies.extend(
            self.jumpers
                .iter()
                .enumerate()
                .map(|(i, j)| (Handle::Jumper(i), j.b)),
        );
        bodies.extend(
            self.drones
                .iter()
                .enumerate()
                .map(|(i, d)| (Handle::Drone(i), d.b)),
        );
        bodies.extend(
            self.collectors
                .iter()
                .enumerate()
                .map(|(i, c)| (Handle::Collector(i), c.b)),
        );
        bodies.extend(
            self.miners
                .iter()
                .enumerate()
                .map(|(i, m)| (Handle::Miner(i), m.b)),
        );
        bodies.extend(
            self.sheriffs
                .iter()
                .enumerate()
                .map(|(i, s)| (Handle::Sheriff(i), s.b)),
        );
        bodies.extend((0..CARRIER_HULL.len()).map(|k| (Handle::Hull(k), hull_body(k))));
        if let Some(r) = &self.refinery {
            bodies.push((Handle::Refinery, r.b));
        }

        for i in 0..bodies.len() {
            let ha = bodies[i].0;
            // As in the browser, `a` is only checked for death once per row.
            if self.is_dead(ha) {
                continue;
            }
            for k in i + 1..bodies.len() {
                let hb = bodies[k].0;
                if self.is_dead(hb) || (bodies[i].1.fixed && bodies[k].1.fixed) {
                    continue;
                }
                if self.passes_carrier(ha, hb) || self.passes_carrier(hb, ha) {
                    continue;
                }
                if only.is_some() && !predicted(ha) && !predicted(hb) {
                    continue;
                }
                let (lo, hi) = bodies.split_at_mut(k);
                if let Some(rel) = collide(&mut lo[i].1, &mut hi[0].1) {
                    self.set_body(ha, bodies[i].1);
                    self.set_body(hb, bodies[k].1);
                    // A tug, or cargo it tows or threw, answers for the impact.
                    if only.is_some() {
                        continue;
                    }
                    let blame = self
                        .impact_blame(ha)
                        .or(self.impact_blame(hb))
                        .unwrap_or_default();
                    self.with_blame(blame, |w| {
                        w.on_impact(ha, hb, rel);
                        w.on_impact(hb, ha, rel);
                    });
                }
            }
        }
        for (h, b) in bodies {
            self.set_body(h, b);
        }
    }

    /// Who answers for an impact by this body: the tug's own pilot, or the
    /// pilot whose cargo it is (towed, or thrown and still flying).
    fn impact_blame(&self, h: Handle) -> Option<crate::world::Blame> {
        match h {
            Handle::Ship(p) => Some(crate::world::Blame::Pilot(p)),
            Handle::Item(i) => self.cargo_blame(i),
            _ => None,
        }
    }

    /// The pilot answering for item `i` while it is on a cable or still
    /// flying from a throw; None for loose cargo.
    pub(crate) fn cargo_blame(&self, i: usize) -> Option<crate::world::Blame> {
        let it = &self.items[i];
        if !(it.towed || it.thrown_t > 0.0) {
            return None;
        }
        Some(
            it.owner
                .map_or(crate::world::Blame::Unset, crate::world::Blame::Pilot),
        )
    }

    /// Damage rules for an impact seen from `me`. This is where "cargo is
    /// your weapon" lives: damage scales with relative speed × mass.
    fn on_impact(&mut self, me: Handle, other: Handle, rel: f64) {
        if self.is_dead(me) || self.is_dead(other) {
            return;
        }
        match me {
            Handle::Ship(p) => {
                if matches!(other, Handle::Hull(_)) || rel < SHIP_IMPACT_SAFE {
                    return;
                }
                // Tugs always push each other, but by the owner's contact
                // rule (2026-11-10) they hurt only between two PvP Enabled
                // pilots; then each answers for the damage to the other.
                if let Handle::Ship(q) = other {
                    let enabled = |k: usize| self.pilots[k].pvp == crate::perm::Pvp::Enabled;
                    if !(enabled(p) && enabled(q)) {
                        return;
                    }
                    self.record_pvp_hit(q, p);
                }
                // Another pilot's towed or thrown cargo hurts this tug only
                // when the permission rule allows it (PvP, see perm.rs).
                if let Handle::Item(i) = other {
                    let by = self.cargo_blame(i).and_then(|b| self.resolve_blame(b));
                    if !self.can_hurt_pilot(by, p) {
                        return;
                    }
                    if let Some(a) = by {
                        self.record_pvp_hit(a, p);
                    }
                }
                let ob = self.body(other);
                let other_mass = if ob.fixed {
                    STATIC_IMPACT_MASS
                } else {
                    ob.mass
                };
                // Your own towed cargo hurts half as much; another pilot's
                // counts as any other cargo.
                let own_cargo = match other {
                    Handle::Item(i) if self.pilots[p].chain.contains(&self.items[i].id) => 0.5,
                    _ => 1.0,
                };
                self.damage_ship(
                    p,
                    (rel - SHIP_IMPACT_SAFE)
                        * SHIP_IMPACT_DAMAGE
                        * (0.4 + other_mass / 10.0).min(1.6)
                        * own_cargo,
                );
            }
            Handle::Item(i) => {
                // Volatile salvage primes on a hard hit instead of dealing damage.
                if self.items[i].def().volatile && rel > VOLATILE_IMPACT {
                    // A hard knock is a pilot's doing only if they towed or threw it.
                    let by = self.cargo_blame(i).unwrap_or(crate::world::Blame::Nobody);
                    self.prime_reactor_by(i, by);
                    return;
                }
                let me_b = self.items[i].b;
                let power = rel * me_b.mass;
                match other {
                    // Below ~140 px/s it is a nudge, not a hit (a jumper
                    // settling in to hook).
                    Handle::Jumper(j) if rel > 140.0 => {
                        self.hurt_jumper(j, power / 260.0, Some(me_b.mass));
                    }
                    Handle::Drone(d) if rel > 140.0 => self.hurt_drone(d, power / 260.0),
                    // Swinging or throwing your cargo into a collector or
                    // the law counts as shooting first; drifting debris doesn't.
                    Handle::Collector(c) if rel > 140.0 => {
                        let it = &self.items[i];
                        if it.towed || it.thrown_t > 0.0 {
                            self.collectors[c].provoked = true;
                        }
                        self.hurt_collector(c, power / 260.0, Some(me_b.mass));
                    }
                    Handle::Sheriff(k) if rel > 140.0 => {
                        let it = &self.items[i];
                        if it.towed || it.thrown_t > 0.0 {
                            self.sheriffs[k].provoked = true;
                        }
                        self.hurt_sheriff(k, power / 260.0, Some(me_b.mass));
                    }
                    Handle::Miner(m) if rel > 140.0 => {
                        let it = &self.items[i];
                        let yours = it.towed || it.thrown_t > 0.0;
                        self.hurt_miner(m, power / 260.0, yours);
                    }
                    Handle::Rock(r) if power > ROCK_IMPACT_POWER => {
                        self.damage_rock(r, (power / ROCK_IMPACT_POWER).floor());
                    }
                    Handle::Wreck(w)
                        if self.wrecks[w].locked && self.items[i].kind == ItemType::Keycard =>
                    {
                        self.unlock_hold(w, i);
                    }
                    Handle::Wreck(w) if power > WRECK_IMPACT_POWER => {
                        self.damage_wreck(w, (power / WRECK_IMPACT_POWER).floor());
                    }
                    _ => {}
                }
                if rel > 150.0 {
                    let o = self.body(other);
                    let (x, y) = ((me_b.x + o.x) / 2.0, (me_b.y + o.y) / 2.0);
                    self.sfx(Sfx::Chip);
                    self.emit(Event::Burst {
                        x,
                        y,
                        n: 4,
                        tint: Tint::Steel,
                        speed: 120.0,
                    });
                }
            }
            Handle::Jumper(j) => {
                if matches!(other, Handle::Rock(_) | Handle::Wreck(_)) && rel > 220.0 {
                    self.without_blame(|w| w.hurt_jumper(j, 1.0, None));
                }
            }
            Handle::Drone(d) => {
                if matches!(other, Handle::Rock(_) | Handle::Wreck(_)) && rel > 220.0 {
                    self.without_blame(|w| w.hurt_drone(d, 1.0));
                }
            }
            Handle::Collector(c) => {
                if matches!(other, Handle::Rock(_) | Handle::Wreck(_)) && rel > 220.0 {
                    self.without_blame(|w| w.hurt_collector(c, 0.5, None));
                }
            }
            Handle::Rock(_)
            | Handle::Wreck(_)
            | Handle::Hull(_)
            | Handle::Miner(_)
            | Handle::Sheriff(_)
            | Handle::Refinery => {}
        }
    }

    /// Damage a rock; it breaks into smaller pieces at zero.
    pub fn damage_rock(&mut self, i: usize, n: f64) {
        self.damage_rock_by(i, n, false);
    }

    /// Damage a rock; `by_miner` is a mining drone's cutter (silent, and a
    /// break throws a debris storm).
    pub(crate) fn damage_rock_by(&mut self, i: usize, n: f64, by_miner: bool) {
        let r = &mut self.rocks[i];
        if r.dead {
            return;
        }
        r.hp -= n;
        r.flash = 0.08;
        if r.hp <= 0.0 {
            self.break_rock(i, by_miner);
        } else if !by_miner {
            self.sfx(Sfx::Chip);
        }
    }

    /// Split a rock into the next size down. Breaking makes the field
    /// busier: everything you disturb makes it more dangerous. A miner's
    /// cut flings the pieces much faster and spills extra ore.
    fn break_rock(&mut self, i: usize, by_miner: bool) {
        self.rocks[i].dead = true;
        let rock = self.rocks[i].clone();
        if by_miner && rock.size >= 2 && self.rng.next() < crate::opposition::MINER_ORE_CHANCE {
            let vx = rock.b.vx + self.rng.range(-50.0, 50.0);
            let vy = rock.b.vy + self.rng.range(-50.0, 50.0);
            let it = self.new_item(ItemType::Ore, rock.b.x, rock.b.y, vx, vy);
            self.items.push(it);
        }
        let kick = if by_miner {
            crate::opposition::MINER_KICK
        } else {
            0.0
        };
        let spawn_child = |w: &mut World, size: u8| {
            let a = w.rng.range(0.0, TAU);
            let sp = w.rng.range(50.0, 110.0) + kick;
            let mut child = make_rock(
                &mut w.rng,
                size,
                rock.b.x + a.cos() * rock.b.r * 0.4,
                rock.b.y + a.sin() * rock.b.r * 0.4,
                rock.b.vx + a.cos() * sp,
                rock.b.vy + a.sin() * sp,
                rock.veined,
            );
            // Pebbles from breakage are temporary, so the field can't silt up.
            if size == 1 {
                child.crumble_t = Some(w.rng.range(PEBBLE_LIFE[0], PEBBLE_LIFE[1]));
            }
            w.rocks.push(child);
        };
        let drop = |w: &mut World, kind: ItemType| {
            let vx = rock.b.vx + w.rng.range(-40.0, 40.0);
            let vy = rock.b.vy + w.rng.range(-40.0, 40.0);
            let it = w.new_item(kind, rock.b.x, rock.b.y, vx, vy);
            w.items.push(it);
        };
        match rock.size {
            3 => {
                for _ in 0..self.rng.int(2, 3) {
                    spawn_child(self, 2);
                }
            }
            2 => {
                spawn_child(self, 1);
                spawn_child(self, 1);
                if rock.veined {
                    drop(self, ItemType::Ore);
                } else if self.rng.chance(0.15) {
                    drop(self, ItemType::Scrap);
                }
            }
            _ => {
                if rock.veined {
                    drop(self, ItemType::Ore);
                } else if self.rng.chance(0.3) {
                    drop(self, ItemType::Scrap);
                }
            }
        }
        self.emit(Event::Dust {
            x: rock.b.x,
            y: rock.b.y,
            r: rock.b.r,
            tint: if rock.veined { Tint::Ore } else { Tint::Rock },
        });
        self.shake = (self.shake + f64::from(rock.size) * 1.5).min(10.0);
        self.sfx(Sfx::Crunch);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn head_on_collision_conserves_momentum() {
        let mut a = Body {
            x: 0.0,
            vx: 100.0,
            r: 10.0,
            mass: 2.0,
            ..Body::default()
        };
        let mut b = Body {
            x: 15.0,
            r: 10.0,
            mass: 2.0,
            ..Body::default()
        };
        let rel = collide(&mut a, &mut b).expect("they overlap and close");
        assert_eq!(rel, 100.0);
        assert!((a.vx * a.mass + b.vx * b.mass - 200.0).abs() < 1e-9);
        assert!(crate::js::dist(a.x, a.y, b.x, b.y) >= 20.0 - 1e-9);
    }
}
