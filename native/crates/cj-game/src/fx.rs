//! Particles and floating text, built from simulation events
//! (addParticle, burst, dustFx, explosionFx and updateEffects in
//! src/js/16-simulation-step.js). Purely visual: their randomness uses the
//! game's own generator so it can never change the simulation.

use crate::look::{col, tint_rgb};
use cj_sim::events::Event;
use cj_sim::rng::Rng;
use cj_sim::World;
use std::f64::consts::PI;

pub const MAX_PARTICLES: usize = 1400;
/// Seconds a laser beam stays on screen.
pub const BEAM_TIME: f64 = 0.09;
const FIRE: [u32; 5] = [0xffffff, 0xfff3b0, 0xffd24a, 0xffb020, 0xff7a3c];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Spark,
    Ring,
    Flash,
    Shard,
    Smoke,
    /// A tumbling piece of broken rock. Reuses two fields: `ring` is its
    /// starting angle and `grow` its spin in radians per second.
    Chunk,
}

/// Seconds a thruster trail point lasts.
pub const ENGINE_TRAIL_TIME: f64 = 0.35;

#[derive(Clone, Debug)]
pub struct Particle {
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
    pub life: f64,
    pub max: f64,
    pub rgb: u32,
    pub size: f64,
    /// Ring radius for shockwaves.
    pub ring: f64,
    pub kind: Kind,
    pub grow: f64,
}

#[derive(Clone, Debug)]
pub struct Popup {
    pub x: f64,
    pub y: f64,
    pub text: String,
    pub rgb: u32,
    pub life: f64,
}

pub struct Fx {
    rng: Rng,
    pub particles: Vec<Particle>,
    pub popups: Vec<Popup>,
    /// Laser beams on screen: (x0, y0, x1, y1, seconds left).
    pub beams: Vec<(f64, f64, f64, f64, f64)>,
    /// Recent positions of fast towed or thrown cargo, by item id (updateTrail).
    pub trails: std::collections::HashMap<u32, std::collections::VecDeque<(f64, f64)>>,
    /// Each pilot's recent stern positions while thrusting, with the world
    /// time they were taken. Expired by age rather than count, so the trail
    /// is the same length whether it's updated every step (offline) or
    /// every frame (online).
    pub engine: Vec<std::collections::VecDeque<(f64, f64, f64)>>,
    /// Explosion flashes lighting the rocks around them: (x, y, reach, rgb,
    /// seconds left, seconds in all).
    pub lights: Vec<(f64, f64, f64, u32, f64, f64)>,
}

/// How far an explosion's flash reaches, as a multiple of its size.
const LIGHT_REACH: f64 = 3.0;

impl Fx {
    pub fn new(rng: Rng) -> Fx {
        Fx {
            rng,
            particles: Vec::new(),
            popups: Vec::new(),
            beams: Vec::new(),
            trails: Default::default(),
            engine: Vec::new(),
            lights: Vec::new(),
        }
    }

    /// The strongest explosion light on a body of radius `r` at (x, y): the
    /// unit direction toward it, its strength (0..1) and its colour.
    pub fn light_at(&self, x: f64, y: f64, r: f64) -> Option<(f64, f64, f64, u32)> {
        let mut best = None;
        let mut top = 0.0;
        for &(lx, ly, reach, rgb, life, max) in &self.lights {
            let (dx, dy) = (lx - x, ly - y);
            let d = dx.hypot(dy);
            let k = (life / max) * (1.0 - (d - r).max(0.0) / reach).clamp(0.0, 1.0);
            if k > top {
                top = k;
                let n = d.max(1e-6);
                best = Some((dx / n, dy / n, k, rgb));
            }
        }
        best
    }

    #[allow(clippy::too_many_arguments)]
    fn add(
        &mut self,
        x: f64,
        y: f64,
        vx: f64,
        vy: f64,
        life: f64,
        rgb: u32,
        size: f64,
        ring: f64,
        kind: Kind,
        grow: f64,
    ) {
        if self.particles.len() >= MAX_PARTICLES {
            self.particles.remove(0);
        }
        self.particles.push(Particle {
            x,
            y,
            vx,
            vy,
            life,
            max: life,
            rgb,
            size,
            ring,
            kind,
            grow,
        });
    }

    /// A short-lived spark. The eight arguments are the particle's own fields; a parameter struct
    /// would only add churn at the dozen call sites, so the lint is allowed like `add` above.
    #[allow(clippy::too_many_arguments)]
    fn spark(&mut self, x: f64, y: f64, vx: f64, vy: f64, life: f64, rgb: u32, size: f64) {
        self.add(x, y, vx, vy, life, rgb, size, 0.0, Kind::Spark, 0.0);
    }

    fn burst(&mut self, x: f64, y: f64, n: f64, rgb: u32, speed: f64) {
        let mut i = 0.0;
        while i < n {
            let a = self.rng.angle();
            let sp = self.rng.range(0.2, 1.0) * speed;
            let life = self.rng.range(0.3, 0.9);
            let size = self.rng.range(1.5, 3.5);
            self.spark(x, y, a.cos() * sp, a.sin() * sp, life, rgb, size);
            i += 1.0;
        }
    }

    fn dust(&mut self, x: f64, y: f64, r: f64, rgb: u32) {
        let mut i = 0.0;
        while i < 3.0 + r / 15.0 {
            let a = self.rng.angle();
            let sp = self.rng.range(10.0, 40.0);
            let px = x + self.rng.range(-0.4, 0.4) * r;
            let py = y + self.rng.range(-0.4, 0.4) * r;
            let life = self.rng.range(0.8, 1.6);
            let size = r * self.rng.range(0.25, 0.45);
            self.add(
                px,
                py,
                a.cos() * sp,
                a.sin() * sp,
                life,
                0x3a4252,
                size,
                0.0,
                Kind::Smoke,
                1.2,
            );
            i += 1.0;
        }
        self.burst(x, y, 6.0 + r / 6.0, rgb, 140.0 + r * 2.0);
        // Tumbling chunks, outlined in the rock's tint (gold for ore).
        let mut i = 0.0;
        while i < 3.0 + r / 12.0 {
            let a = self.rng.angle();
            let sp = self.rng.range(60.0, 160.0) + r;
            let px = x + a.cos() * r * 0.4;
            let py = y + a.sin() * r * 0.4;
            let life = self.rng.range(0.9, 1.6);
            let size = r * self.rng.range(0.12, 0.22);
            let start = self.rng.angle();
            let spin = self.rng.range(-6.0, 6.0);
            self.add(
                px,
                py,
                a.cos() * sp,
                a.sin() * sp,
                life,
                rgb,
                size,
                start,
                Kind::Chunk,
                spin,
            );
            i += 1.0;
        }
    }

    fn explosion(&mut self, x: f64, y: f64, size: f64, rgb: u32) {
        let k = size / 100.0;
        self.add(
            x,
            y,
            0.0,
            0.0,
            0.3,
            0xffffff,
            size * 1.1,
            0.0,
            Kind::Flash,
            0.8,
        );
        self.add(x, y, 0.0, 0.0, 0.65, rgb, size * 1.6, 0.0, Kind::Flash, 0.6);
        for _ in 0..(30.0 + 50.0 * k).round() as usize {
            let a = self.rng.angle();
            let sp = self.rng.range(0.15, 1.0) * size * 4.0;
            let life = self.rng.range(0.45, 1.2) * (0.8 + k * 0.25);
            let c = if self.rng.chance(0.25) {
                rgb
            } else {
                FIRE[self.rng.index(FIRE.len())]
            };
            let sz = self.rng.range(2.0, 4.5) * (0.85 + k * 0.3);
            self.spark(x, y, a.cos() * sp, a.sin() * sp, life, c, sz);
        }
        let mut i = 0.0;
        while i < 6.0 + 10.0 * k {
            let a = self.rng.angle();
            let sp = self.rng.range(0.5, 1.0) * size * 5.5;
            let life = self.rng.range(0.3, 0.6);
            let c = FIRE[self.rng.index(FIRE.len())];
            self.add(
                x,
                y,
                a.cos() * sp,
                a.sin() * sp,
                life,
                c,
                2.0,
                0.0,
                Kind::Shard,
                0.0,
            );
            i += 1.0;
        }
        let mut i = 0.0;
        while i < 4.0 + 6.0 * k {
            let a = self.rng.angle();
            let sp = self.rng.range(10.0, 50.0);
            let px = x + self.rng.range(-0.3, 0.3) * size;
            let py = y + self.rng.range(-0.3, 0.3) * size;
            let life = self.rng.range(1.2, 2.2);
            let sz = size * self.rng.range(0.18, 0.32);
            self.add(
                px,
                py,
                a.cos() * sp,
                a.sin() * sp,
                life,
                0x2a2f3a,
                sz,
                0.0,
                Kind::Smoke,
                1.6,
            );
            i += 1.0;
        }
        self.add(x, y, 0.0, 0.0, 0.6, rgb, 6.0, size * 2.1, Kind::Ring, 0.0);
        self.add(
            x,
            y,
            0.0,
            0.0,
            0.35,
            0xffffff,
            3.0,
            size * 1.3,
            Kind::Ring,
            0.0,
        );
    }

    /// Turn this step's simulation events into particles and popups.
    /// Sound and radio events are left for the audio and radio layers.
    pub fn absorb<'a>(&mut self, events: impl IntoIterator<Item = &'a Event>) {
        for e in events {
            match e {
                Event::Burst {
                    x,
                    y,
                    n,
                    tint,
                    speed,
                } => self.burst(*x, *y, f64::from(*n), tint_rgb(*tint), *speed),
                Event::Dust { x, y, r, tint } => self.dust(*x, *y, *r, tint_rgb(*tint)),
                // An EMP pulse: an electric ring out to its reach, and a slower echo.
                Event::Emp { x, y, r } => {
                    self.add(*x, *y, 0.0, 0.0, 0.5, col::CABLE, 3.0, *r, Kind::Ring, 0.0);
                    self.add(
                        *x,
                        *y,
                        0.0,
                        0.0,
                        0.9,
                        col::SHIELD,
                        2.0,
                        r * 0.75,
                        Kind::Ring,
                        0.0,
                    );
                }
                Event::Explosion { x, y, size, tint } => {
                    // Bigger blasts light the field for longer.
                    let life = 0.25 + size / 400.0;
                    let light = (*x, *y, size * LIGHT_REACH, tint_rgb(*tint), life, life);
                    self.lights.push(light);
                    self.explosion(*x, *y, *size, tint_rgb(*tint))
                }
                Event::Popup { x, y, text, tint } => self.popups.push(Popup {
                    x: *x,
                    y: *y,
                    text: text.clone(),
                    rgb: tint_rgb(*tint),
                    life: 1.4,
                }),
                Event::Laser { x0, y0, x1, y1 } => {
                    self.beams.push((*x0, *y0, *x1, *y1, BEAM_TIME));
                }
                // A round striking something: a brief hot flash.
                Event::Hit { x, y } => self.spark(*x, *y, 0.0, 0.0, 0.15, col::HAZARD, 3.0),
                Event::Strafe {
                    x,
                    y,
                    vx,
                    vy,
                    px,
                    py,
                    side,
                } if self.rng.chance(0.7) => {
                    let (ox, oy) = (px * 17.0 * side, py * 17.0 * side);
                    let (bx, by) = (px * side * 160.0, py * side * 160.0);
                    self.spark(x + ox, y + oy, vx + bx, vy + by, 0.25, col::CABLE, 2.5);
                }
                _ => {}
            }
        }
    }

    /// Per-step visual emitters that the browser runs inside gameplay code:
    /// engine exhaust and rare-item glints.
    /// Keep a short position history on fast cargo so swings and throws
    /// read: cargo faster than 220 px/s keeps its last 8 positions, and a
    /// slowed piece's trail shrinks a point a step.
    pub fn update_trails(&mut self, w: &World) {
        const TRAIL_SPEED: f64 = 220.0;
        const TRAIL_LEN: usize = 8;
        self.trails
            .retain(|id, _| w.items.iter().any(|it| it.id == *id && !it.dead));
        for it in &w.items {
            let fast = (it.towed || it.thrown_t > 0.0) && it.b.vx.hypot(it.b.vy) > TRAIL_SPEED;
            if fast {
                let tr = self.trails.entry(it.id).or_default();
                tr.push_back((it.b.x, it.b.y));
                if tr.len() > TRAIL_LEN {
                    tr.pop_front();
                }
            } else if let Some(tr) = self.trails.get_mut(&it.id) {
                tr.pop_front();
            }
        }
        // Thruster trails: a point per update while the engine burns
        // cleanly, dropped once older than ENGINE_TRAIL_TIME. A wreck or a
        // clock that went backwards (a new shift) clears the trail.
        self.engine.resize_with(w.pilots.len(), Default::default);
        for (tr, pl) in self.engine.iter_mut().zip(&w.pilots) {
            let s = &pl.ship;
            if !s.alive {
                tr.clear();
                continue;
            }
            if s.throttle > 0.0 && !s.sputtering {
                let (x, y) = s.stern();
                tr.push_back((x, y, w.t));
            }
            while tr
                .front()
                .is_some_and(|q| w.t - q.2 > ENGINE_TRAIL_TIME || q.2 > w.t)
            {
                tr.pop_front();
            }
        }
    }

    pub fn emit_ambient(&mut self, w: &World) {
        let s = &w.me().ship;
        // Exhaust: visibly choked while sputtering. Only forward thrust smokes.
        if s.alive && s.throttle > 0.0 && self.rng.chance(if s.sputtering { 0.25 } else { 0.9 }) {
            let bx = s.b.x - s.angle.cos() * 20.0;
            let by = s.b.y - s.angle.sin() * 20.0;
            let sp = self.rng.range(140.0, 240.0);
            let a = s.angle + PI + self.rng.range(-0.25, 0.25);
            let life = self.rng.range(0.2, 0.4);
            let size = self.rng.range(2.0, 4.0);
            let c = if s.sputtering { 0x888888 } else { col::HAZARD };
            self.spark(
                bx,
                by,
                s.b.vx + a.cos() * sp,
                s.b.vy + a.sin() * sp,
                life,
                c,
                size,
            );
        }
        // Stunned by a sheriff bolt: blue crackle round the hull.
        if s.alive && s.stun_t > 0.0 && self.rng.chance(0.5) {
            let x = s.b.x + self.rng.range(-18.0, 18.0);
            let y = s.b.y + self.rng.range(-18.0, 18.0);
            let (vx, vy) = (self.rng.range(-40.0, 40.0), self.rng.range(-40.0, 40.0));
            self.spark(x, y, vx, vy, 0.25, col::LAW, 2.0);
        }
        // A cooked engine smokes while it cools.
        if s.alive && s.engine_over_t > 0.0 && self.rng.chance(0.5) {
            let x = s.b.x + self.rng.range(-12.0, 12.0);
            let y = s.b.y + self.rng.range(-12.0, 12.0);
            let (vx, vy) = (self.rng.range(-30.0, 30.0), self.rng.range(-30.0, 30.0));
            self.spark(x, y, vx, vy, 0.6, col::MUTED, 3.0);
        }
        // Missile exhaust trails.
        for m in &w.missiles {
            if m.life > 0.0 && self.rng.chance(0.8) {
                let vx = -m.vx * 0.1 + self.rng.range(-20.0, 20.0);
                let vy = -m.vy * 0.1 + self.rng.range(-20.0, 20.0);
                self.spark(m.x, m.y, vx, vy, 0.35, col::MUTED, 2.0);
            }
        }
        // Smoke from the refinery's broken jaws.
        if let Some(r) = w
            .refinery
            .as_ref()
            .filter(|r| r.state == cj_sim::opposition::RefineryState::Wrecked)
        {
            if self.rng.chance(0.4) {
                let (mx, my) = r.maw();
                let x = mx + self.rng.range(-40.0, 40.0);
                let y = my + self.rng.range(-40.0, 40.0);
                let (vx, vy) = (self.rng.range(-20.0, 20.0), self.rng.range(-40.0, -10.0));
                self.spark(x, y, vx, vy, 1.0, col::MUTED, 3.0);
            }
        }
        for i in 0..w.items.len() {
            let it = &w.items[i];
            // Rare grades glint now and then so they stand out across the field.
            if it.rare && self.rng.chance(0.06) {
                let (x, y, r) = (it.b.x, it.b.y, it.b.r);
                let c = it.tint.unwrap_or(col::TEXT);
                let px = x + self.rng.range(-r, r);
                let py = y + self.rng.range(-r, r);
                self.spark(px, py, 0.0, -12.0, 0.5, c, 2.0);
            }
            // Thrown cargo leaves a short streak so the player can read the throw.
            if it.thrown_t > 0.0 && it.b.speed() > 180.0 && self.rng.chance(0.5) {
                self.spark(it.b.x, it.b.y, 0.0, 0.0, 0.3, col::CABLE, 2.0);
            }
        }
    }

    /// Advance particles and popups by `dt` (updateEffects).
    pub fn update(&mut self, dt: f64) {
        for p in &mut self.particles {
            p.x += p.vx * dt;
            p.y += p.vy * dt;
            let drag = if p.kind == Kind::Smoke { 0.94 } else { 0.97 };
            p.vx *= drag;
            p.vy *= drag;
            p.life -= dt;
        }
        self.particles.retain(|p| p.life > 0.0);
        for p in &mut self.popups {
            p.y -= 34.0 * dt;
            p.life -= dt;
        }
        self.popups.retain(|p| p.life > 0.0);
        for l in &mut self.lights {
            l.4 -= dt;
        }
        self.lights.retain(|l| l.4 > 0.0);
        for b in &mut self.beams {
            b.4 -= dt;
        }
        self.beams.retain(|b| b.4 > 0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An explosion lights rocks within its reach, from its side, and the
    /// light fades out within a second.
    #[test]
    fn explosions_light_nearby_rocks_briefly() {
        let mut fx = Fx::new(Rng::seeded(3));
        let boom = Event::Explosion {
            x: 100.0,
            y: 0.0,
            size: 45.0,
            tint: cj_sim::events::Tint::Hazard,
        };
        fx.absorb([&boom]);
        let (dx, _, k, _) = fx.light_at(0.0, 0.0, 20.0).expect("lit");
        assert!(dx > 0.99 && k > 0.4, "{dx} {k}");
        assert!(fx.light_at(1000.0, 0.0, 20.0).is_none());
        fx.update(1.0);
        assert!(fx.light_at(0.0, 0.0, 20.0).is_none());
    }

    /// The thruster trail grows while the engine burns, drops points older
    /// than ENGINE_TRAIL_TIME, and clears when the tug is wrecked or the
    /// clock goes back (a new shift).
    #[test]
    fn engine_trail_expires_by_age() {
        let mut w = World::new_shift(Rng::seeded(1), (1280.0, 720.0));
        let mut fx = Fx::new(Rng::seeded(2));
        w.pilots[0].ship.alive = true;
        w.pilots[0].ship.sputtering = false;
        w.pilots[0].ship.throttle = 1.0;
        for i in 0..60 {
            w.t = f64::from(i) / 60.0;
            fx.update_trails(&w);
        }
        let tr = &fx.engine[0];
        // 0.35 s at 60 updates a second: 21 or 22 points, all recent.
        assert!((20..=22).contains(&tr.len()), "{}", tr.len());
        assert!(tr.iter().all(|q| w.t - q.2 <= ENGINE_TRAIL_TIME));

        // Engine off: nothing new, and the trail runs out.
        w.pilots[0].ship.throttle = 0.0;
        w.t += ENGINE_TRAIL_TIME + 0.01;
        fx.update_trails(&w);
        assert!(fx.engine[0].is_empty());

        // A new shift's clock starts again from zero: old points go.
        w.pilots[0].ship.throttle = 1.0;
        fx.update_trails(&w);
        w.t = 0.0;
        fx.update_trails(&w);
        assert_eq!(fx.engine[0].len(), 1);

        // Wrecked: cleared at once.
        w.pilots[0].ship.alive = false;
        fx.update_trails(&w);
        assert!(fx.engine[0].is_empty());
    }
}
