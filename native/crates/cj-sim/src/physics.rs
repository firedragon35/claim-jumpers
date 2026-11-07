//! Integration, circle collisions and impact damage (integrateAll,
//! collide, onImpact and resolveCollisions in src/js/16-simulation-step.js).

use crate::consts::*;
use crate::entities::{make_rock, Body, JumperState};
use crate::events::{Event, Sfx, Tint};
use crate::items::ItemType;
use crate::world::World;
use std::f64::consts::TAU;

/// Which entity a collision body belongs to.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Handle {
    Ship,
    Rock(usize),
    Item(usize),
    Wreck(usize),
    Jumper(usize),
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
fn integrate(b: &mut Body, dt: f64, damp: f64, bounded: bool) {
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
    if b.x < m {
        b.vx += (m - b.x) * push * dt;
    }
    if b.x > WORLD_W - m {
        b.vx -= (b.x - (WORLD_W - m)) * push * dt;
    }
    if b.y < m {
        b.vy += (m - b.y) * push * dt;
    }
    if b.y > WORLD_H - m {
        b.vy -= (b.y - (WORLD_H - m)) * push * dt;
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
    let d = match d2.sqrt() {
        v if v == 0.0 => 0.001,
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
    pub(crate) fn integrate_all(&mut self, dt: f64) {
        if self.ship.alive {
            // (A netted tug has no wall: milestone 4.)
            integrate(&mut self.ship.b, dt, SHIP_DAMP, true);
        }
        for r in &mut self.rocks {
            // Storm debris settles over a few seconds; normal drift is undamped.
            let damp = if r.b.speed() > STORM_SPEED {
                STORM_DAMP
            } else {
                0.0
            };
            integrate(&mut r.b, dt, damp, true);
        }
        // A rival-hauled item may be dragged out of the field.
        for it in &mut self.items {
            let bounded = it.hooked_by.is_none();
            integrate(&mut it.b, dt, ITEM_DAMP, bounded);
        }
        for w in &mut self.wrecks {
            integrate(&mut w.b, dt, WRECK_DAMP, true);
        }
        for j in &mut self.jumpers {
            let bounded = j.state != JumperState::Haul;
            integrate(&mut j.b, dt, JUMPER_DAMP, bounded);
        }
        // (Drones, collectors, miners and sheriffs: milestone 4.)
    }

    fn body(&self, h: Handle) -> Body {
        match h {
            Handle::Ship => self.ship.b,
            Handle::Rock(i) => self.rocks[i].b,
            Handle::Item(i) => self.items[i].b,
            Handle::Wreck(i) => self.wrecks[i].b,
            Handle::Jumper(i) => self.jumpers[i].b,
            Handle::Hull(k) => hull_body(k),
        }
    }

    fn set_body(&mut self, h: Handle, b: Body) {
        match h {
            Handle::Ship => self.ship.b = b,
            Handle::Rock(i) => self.rocks[i].b = b,
            Handle::Item(i) => self.items[i].b = b,
            Handle::Wreck(i) => self.wrecks[i].b = b,
            Handle::Jumper(i) => self.jumpers[i].b = b,
            Handle::Hull(_) => {}
        }
    }

    fn is_dead(&self, h: Handle) -> bool {
        match h {
            Handle::Rock(i) => self.rocks[i].dead,
            Handle::Item(i) => self.items[i].dead,
            Handle::Wreck(i) => self.wrecks[i].dead,
            Handle::Jumper(i) => self.jumpers[i].dead,
            Handle::Ship | Handle::Hull(_) => false,
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
        let mut bodies: Vec<(Handle, Body)> = Vec::new();
        if self.ship.alive {
            bodies.push((Handle::Ship, self.ship.b));
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
        // (Drones, collectors, miners, sheriffs and the refinery body go
        // between the jumpers and the hull in milestone 4.)
        bodies.extend((0..CARRIER_HULL.len()).map(|k| (Handle::Hull(k), hull_body(k))));

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
                let (lo, hi) = bodies.split_at_mut(k);
                if let Some(rel) = collide(&mut lo[i].1, &mut hi[0].1) {
                    self.set_body(ha, bodies[i].1);
                    self.set_body(hb, bodies[k].1);
                    self.on_impact(ha, hb, rel);
                    self.on_impact(hb, ha, rel);
                }
            }
        }
        for (h, b) in bodies {
            self.set_body(h, b);
        }
    }

    /// Damage rules for an impact seen from `me`. This is where "cargo is
    /// your weapon" lives: damage scales with relative speed × mass.
    fn on_impact(&mut self, me: Handle, other: Handle, rel: f64) {
        if self.is_dead(me) || self.is_dead(other) {
            return;
        }
        match me {
            Handle::Ship => {
                if matches!(other, Handle::Hull(_)) || rel < SHIP_IMPACT_SAFE {
                    return;
                }
                let ob = self.body(other);
                let other_mass = if ob.fixed {
                    STATIC_IMPACT_MASS
                } else {
                    ob.mass
                };
                // Your own towed cargo hurts half as much.
                let own_cargo = match other {
                    Handle::Item(i) if self.items[i].towed => 0.5,
                    _ => 1.0,
                };
                self.damage_ship(
                    (rel - SHIP_IMPACT_SAFE)
                        * SHIP_IMPACT_DAMAGE
                        * (0.4 + other_mass / 10.0).min(1.6)
                        * own_cargo,
                );
            }
            Handle::Item(i) => {
                // Volatile salvage primes on a hard hit instead of dealing damage.
                if self.items[i].def().volatile && rel > VOLATILE_IMPACT {
                    self.prime_reactor(i);
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
                    // (Drones, collectors, the law and miners: milestone 4.)
                    Handle::Rock(r) if power > ROCK_IMPACT_POWER => {
                        self.damage_rock(r, (power / ROCK_IMPACT_POWER).floor());
                    }
                    // (Keycards opening sealed holds: milestone 4.)
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
                    self.hurt_jumper(j, 1.0, None);
                }
            }
            Handle::Rock(_) | Handle::Wreck(_) | Handle::Hull(_) => {}
        }
    }

    /// Damage a rock; it breaks into smaller pieces at zero.
    pub fn damage_rock(&mut self, i: usize, n: f64) {
        let r = &mut self.rocks[i];
        if r.dead {
            return;
        }
        r.hp -= n;
        r.flash = 0.08;
        if r.hp <= 0.0 {
            self.break_rock(i);
        } else {
            self.sfx(Sfx::Chip);
        }
    }

    /// Split a rock into the next size down. Breaking makes the field
    /// busier: everything you disturb makes it more dangerous.
    fn break_rock(&mut self, i: usize) {
        self.rocks[i].dead = true;
        let rock = self.rocks[i].clone();
        // (Miner cuts fling pieces faster and spill extra ore: milestone 4.)
        let spawn_child = |w: &mut World, size: u8| {
            let a = w.rng.range(0.0, TAU);
            let sp = w.rng.range(50.0, 110.0);
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
