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
}

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
}

impl Fx {
    pub fn new(rng: Rng) -> Fx {
        Fx {
            rng,
            particles: Vec::new(),
            popups: Vec::new(),
            beams: Vec::new(),
            trails: Default::default(),
        }
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
                Event::Explosion { x, y, size, tint } => {
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
        for b in &mut self.beams {
            b.4 -= dt;
        }
        self.beams.retain(|b| b.4 > 0.0);
    }
}
