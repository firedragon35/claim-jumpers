//! Local co-op (src/js/10e-modes.js): a second player flies a partner tug.
//! It can latch onto the tail of the cable so two tugs haul one chain, and
//! it has a light cutter. It can't be wrecked (a support role) and shares
//! the shift's bank.

use crate::cable::rope_constraint;
use crate::consts::{BAY_X, BAY_Y, WORLD_H, WORLD_W};
use crate::entities::{Body, Bullet};
use crate::events::{Radio, Sfx};
use crate::js;
use crate::world::World;
use std::f64::consts::PI;

pub const P2_THRUST: f64 = 430.0;
pub const P2_TURN: f64 = 3.6;
pub const P2_MAX_SPEED: f64 = 520.0;
pub const P2_MASS: f64 = 3.0;
/// Distance to the cable's tail that allows latching.
pub const P2_LATCH_RANGE: f64 = 160.0;
/// Seconds between cutter shots.
pub const P2_FIRE: f64 = 0.22;
pub const P2_DAMAGE: f64 = 0.7;
/// The latch breaks if stretched past this × its length.
pub const P2_BREAK: f64 = 2.2;

/// P2's controls this step (readP2); `latch` is the button held.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct P2Input {
    pub thrust: f64,
    pub brake: f64,
    pub turn: f64,
    pub fire: bool,
    pub latch: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Partner {
    pub b: Body,
    pub angle: f64,
    pub linked: bool,
    pub link: f64,
    pub fire_t: f64,
    pub thrust: f64,
    prev_latch: bool,
}

impl World {
    pub fn spawn_partner(&mut self) {
        self.p2 = Some(Partner {
            b: Body {
                x: BAY_X - 120.0,
                y: BAY_Y + 60.0,
                vx: 0.0,
                vy: 0.0,
                r: 12.0,
                mass: P2_MASS,
                fixed: false,
            },
            angle: PI,
            linked: false,
            link: 0.0,
            fire_t: 0.0,
            thrust: 0.0,
            prev_latch: false,
        });
    }

    /// The cable's last piece (the tail P2 latches onto), as an item index.
    fn tail_index(&self) -> Option<usize> {
        self.chain.last().and_then(|id| self.item_index(*id))
    }

    pub(crate) fn update_partner(&mut self, dt: f64) {
        let inp = self.p2_input;
        let Some(p) = self.p2.as_mut() else { return };
        p.angle += inp.turn * P2_TURN * dt;
        p.thrust = inp.thrust;
        p.b.vx += p.angle.cos() * P2_THRUST * inp.thrust * dt;
        p.b.vy += p.angle.sin() * P2_THRUST * inp.thrust * dt;
        let drag = (-(0.3 + inp.brake * 2.5) * dt).exp();
        p.b.vx *= drag;
        p.b.vy *= drag;
        let sp = js::hypot(p.b.vx, p.b.vy);
        if sp > P2_MAX_SPEED {
            p.b.vx *= P2_MAX_SPEED / sp;
            p.b.vy *= P2_MAX_SPEED / sp;
        }
        p.b.x = (p.b.x + p.b.vx * dt).clamp(20.0, WORLD_W - 20.0);
        p.b.y = (p.b.y + p.b.vy * dt).clamp(20.0, WORLD_H - 20.0);
        // Bounce off rocks rather than pass through them.
        for r in &self.rocks {
            let (dx, dy) = (p.b.x - r.b.x, p.b.y - r.b.y);
            let d = js::hypot(dx, dy);
            let d = if d == 0.0 { 1.0 } else { d };
            let min = r.b.r + p.b.r;
            if d >= min {
                continue;
            }
            let (nx, ny) = (dx / d, dy / d);
            p.b.x = r.b.x + nx * min;
            p.b.y = r.b.y + ny * min;
            let vn = p.b.vx * nx + p.b.vy * ny;
            if vn < 0.0 {
                p.b.vx -= 1.5 * vn * nx;
                p.b.vy -= 1.5 * vn * ny;
            }
        }
        // Light cutter: counts as your fire (it's your crew).
        p.fire_t -= dt;
        let mut shot = None;
        if inp.fire && p.fire_t <= 0.0 {
            p.fire_t = P2_FIRE;
            let (c, s) = (p.angle.cos(), p.angle.sin());
            shot = Some(Bullet {
                x: p.b.x + c * 16.0,
                y: p.b.y + s * 16.0,
                vx: p.b.vx + c * 820.0,
                vy: p.b.vy + s * 820.0,
                life: 0.75,
                dmg: P2_DAMAGE,
                ..Bullet::default()
            });
        }
        // Latch toggles on the button's rising edge.
        let toggle = inp.latch && !p.prev_latch;
        p.prev_latch = inp.latch;
        if let Some(b) = shot {
            self.bullets.push(b);
            self.sfx(Sfx::SentryShot);
        }
        if toggle {
            self.toggle_partner_latch();
        }
        let tail = self.tail_index().map(|i| self.items[i].b);
        let p = self.p2.as_mut().expect("partner");
        let stretched = tail.is_none_or(|t| js::dist(t.x, t.y, p.b.x, p.b.y) > p.link * P2_BREAK);
        if p.linked && stretched {
            p.linked = false;
            let text = "P2, you lost the cable.".to_owned();
            self.radio_say(
                Radio::P2Free,
                false,
                false,
                text,
                "P2 lost the cable.".to_owned(),
            );
        }
    }

    fn toggle_partner_latch(&mut self) {
        let tail = self.tail_index().map(|i| self.items[i].b);
        let p = self.p2.as_mut().expect("partner");
        if p.linked {
            p.linked = false;
            self.sfx(Sfx::Drop);
            return;
        }
        let Some(t) = tail else { return };
        let d = js::dist(t.x, t.y, p.b.x, p.b.y);
        if d > P2_LATCH_RANGE {
            return;
        }
        p.linked = true;
        p.link = d + 4.0;
        self.sfx(Sfx::Hook);
        let text = "P2 has the tail of the cable. Two tugs, one haul. Try not to rip it in half.";
        self.radio_say(
            Radio::P2Latch,
            false,
            false,
            text.to_owned(),
            "P2 has the tail.".to_owned(),
        );
    }

    /// Rope between the cable's last piece and the partner (one solver pass).
    pub(crate) fn partner_constraint(&mut self) {
        let Some(i) = self.tail_index() else { return };
        if let Some(p) = self.p2.as_mut().filter(|p| p.linked) {
            rope_constraint(&mut self.items[i].b, &mut p.b, p.link);
        }
    }
}
