//! The tug and its mounted hardware (drawTug in src/js/17-rendering.js),
//! drawn in flight and, larger, on the machine shop's preview bench.
//! Local coordinates have the nose along +x, as in the browser.

use crate::draw::{circle_pts, fill_fan, fill_poly, glow_stroke, polyline};
use crate::look::{col, rgba, Look};
use cj_sim::mods::Mods;
use cj_sim::parts::{SENTRY_MAX, TUG_CLASSES};
use cj_sim::rng::Rng;
use macroquad::prelude::*;
use std::f32::consts::{PI, TAU};

/// A local-to-screen transform with a uniform scale (the shop preview
/// draws the tug several times larger than in flight).
#[derive(Clone, Copy)]
pub struct Frame {
    o: Vec2,
    c: f32,
    s: f32,
    k: f32,
}

impl Frame {
    pub fn new(x: f32, y: f32, angle: f32, scale: f32) -> Frame {
        Frame {
            o: vec2(x, y),
            c: angle.cos(),
            s: angle.sin(),
            k: scale,
        }
    }
    fn p(&self, x: f32, y: f32) -> Vec2 {
        let (x, y) = (x * self.k, y * self.k);
        self.o + vec2(x * self.c - y * self.s, x * self.s + y * self.c)
    }
    fn pts(&self, local: &[(f32, f32)]) -> Vec<Vec2> {
        local.iter().map(|&(x, y)| self.p(x, y)).collect()
    }
    fn rect(&self, x: f32, y: f32, w: f32, h: f32) -> Vec<Vec2> {
        self.pts(&[(x, y), (x + w, y), (x + w, y + h), (x, y + h)])
    }
    fn arc(&self, cx: f32, cy: f32, r: f32, a0: f32, a1: f32, n: usize) -> Vec<Vec2> {
        (0..=n)
            .map(|i| {
                let a = a0 + (a1 - a0) * i as f32 / n as f32;
                self.p(cx + a.cos() * r, cy + a.sin() * r)
            })
            .collect()
    }
}

/// What the tug is doing this frame (drawTug's `st`).
#[derive(Clone, Copy, Default)]
pub struct TugState {
    pub t: f32,
    pub thrust: f32,
    pub sputtering: bool,
    /// Side-thruster input, -1..1.
    pub strafe: f32,
    pub flash: bool,
    pub overheated: bool,
    /// Sentries aboard (shown on the drone bay in the shop).
    pub sentries: u32,
    /// The afterburner is firing (a long blue flame).
    pub burn: bool,
    /// Missiles in the rack (None shows a full rack, as in the shop).
    pub missiles: Option<u32>,
}

fn fill_rect(f: &Frame, x: f32, y: f32, w: f32, h: f32, c: Color) {
    let r = f.rect(x, y, w, h);
    fill_fan((r[0] + r[2]) / 2.0, &r, c);
}

pub fn draw_tug(look: &Look, f: &Frame, m: &Mods, st: &TugState, rng: &mut Rng) {
    let k = f.k;
    let t = st.t;
    // Armor: an outer plate shell drawn behind the hull.
    if m.armor {
        let shell = f.pts(&[
            (25.0, -9.0),
            (25.0, 9.0),
            (16.0, 17.0),
            (-16.0, 18.0),
            (-23.0, 11.0),
            (-23.0, -11.0),
            (-16.0, -18.0),
            (16.0, -17.0),
        ]);
        fill_fan(f.o, &shell, Color::from_hex(0x3a3f4a));
        polyline(&shell, true, 1.5 * k, Color::from_hex(col::STEEL));
        for x in [-12.0, 0.0, 12.0] {
            fill_rect(f, x - 1.0, -17.0, 2.0, 2.0, Color::from_hex(col::STEEL));
            fill_rect(f, x - 1.0, 15.0, 2.0, 2.0, Color::from_hex(col::STEEL));
        }
    }
    // Side thruster pods on both flanks, puffing on the side opposite the strafe.
    if m.thrusters {
        for side in [-1.0f32, 1.0] {
            let pod = f.rect(-8.0, side * 15.0 - 3.0 + side * 3.0, 14.0, 6.0);
            fill_fan((pod[0] + pod[2]) / 2.0, &pod, Color::from_hex(0x16303a));
            polyline(&pod, true, 1.5 * k, Color::from_hex(col::CABLE));
            if st.strafe != 0.0 && st.strafe.signum() == -side {
                let reach = rng.range(4.0, 10.0) as f32;
                draw_triangle(
                    f.p(-4.0, side * 21.0),
                    f.p(-1.0, side * (26.0 + reach)),
                    f.p(2.0, side * 21.0),
                    Color::from_hex(col::CABLE),
                );
            }
        }
    }
    // Exhaust flame, choked while sputtering; the afterburner throws a
    // long, blue-hot flame.
    if st.thrust > 0.0 && !st.sputtering {
        let len = rng.range(14.0, 26.0) as f32 * if st.burn { 2.0 } else { 1.0 };
        let flame = if st.burn { col::CABLE } else { col::HAZARD };
        let g = f.p(-24.0, 0.0);
        look.additive(|| look.glow(g.x, g.y, (16.0 + len * 0.4) * k, flame, true, 1.0));
        draw_triangle(
            f.p(-20.0, -6.0),
            f.p(-20.0 - len, 0.0),
            f.p(-20.0, 6.0),
            rgba(flame, 0.85),
        );
        draw_triangle(
            f.p(-20.0, -3.0),
            f.p(-20.0 - len * 0.55, 0.0),
            f.p(-20.0, 3.0),
            Color::from_hex(0xfff3d0),
        );
    }
    // Gun barrels, red while the guns are overheated.
    let barrel_c = Color::from_hex(if st.overheated {
        col::DANGER
    } else {
        col::TEXT
    });
    for &y in &m.barrels {
        fill_rect(f, 20.0, y as f32 - 1.5, 12.0, 3.0, barrel_c);
    }
    // Magnetic scoop: a horseshoe magnet under the nose with a pulsing field.
    if m.scoop {
        let horseshoe = f.arc(24.0, 0.0, 9.0, -PI / 2.0 - 0.9, -PI / 2.0 + 0.9 - PI, 16);
        polyline(&horseshoe, false, 3.0 * k, Color::from_hex(col::DANGER));
        let a = 0.35 + (t * 6.0).sin() * 0.2;
        let field = f.arc(28.0, 0.0, 16.0 + (t * 20.0) % 10.0, -0.9, 0.9, 16);
        polyline(&field, false, k, rgba(col::CABLE, a));
    }
    // Class hardware behind the hull: each trade-up adds a visible feature.
    // (Custom paint jobs arrive with records in milestone 5.)
    let tier = m.tug_class;
    let cls = &TUG_CLASSES[tier];
    let (hull_c, trim_c) = (Color::from_hex(cls.hull), Color::from_hex(cls.trim));
    let hardware = |pts: Vec<Vec2>| {
        fill_poly(&pts, hull_c);
        polyline(&pts, true, 1.5 * k, trim_c);
    };
    for side in [-1.0f32, 1.0] {
        if tier >= 1 {
            // Mk II+: swept tail fins.
            hardware(f.pts(&[
                (-10.0, side * 13.0),
                (-26.0, side * 22.0),
                (-20.0, side * 9.0),
            ]));
        }
        if tier >= 2 {
            // Mk III+: engine nacelles.
            hardware(f.rect(-22.0, side * 16.0 - 3.0, 20.0, 6.0));
        }
        if tier >= 3 {
            // Mk IV+: forward canards.
            hardware(f.pts(&[(16.0, side * 12.0), (8.0, side * 21.0), (2.0, side * 13.0)]));
        }
    }
    // Hull.
    let hull = f.pts(&[
        (22.0, -6.0),
        (22.0, 6.0),
        (14.0, 13.0),
        (-14.0, 14.0),
        (-20.0, 8.0),
        (-20.0, -8.0),
        (-14.0, -14.0),
        (14.0, -13.0),
    ]);
    fill_fan(f.o, &hull, hull_c);
    let edge = if st.flash { col::DANGER } else { col::HAZARD };
    glow_stroke(&hull, true, edge, 2.0 * k, 1.0);
    if tier >= 1 {
        // Class stripe.
        fill_rect(f, -14.0, -2.0, 18.0, 4.0, trim_c);
    }
    if tier >= 4 {
        // Mk V: a hot reactor core amidships.
        let c = f.p(-2.0, 0.0);
        look.additive(|| {
            look.glow(
                c.x,
                c.y,
                (12.0 + (t * 5.0).sin() * 2.0) * k,
                cls.trim,
                true,
                1.0,
            )
        });
    }
    fill_rect(f, 6.0, -5.0, 9.0, 10.0, Color::from_hex(col::CABLE)); // cab window
    let amber = Color::from_hex(col::HAZARD);
    polyline(
        &f.pts(&[(-6.0, -14.0), (-6.0, 14.0)]),
        false,
        1.5 * k,
        amber,
    );
    // Winch: the reinforced version is a bigger drum on an A-frame.
    if m.winch {
        polyline(
            &f.pts(&[(-10.0, -12.0), (-24.0, 0.0), (-10.0, 12.0)]),
            false,
            2.5 * k,
            amber,
        );
        polyline(&f.arc(-17.0, 0.0, 7.0, 0.0, TAU, 24), true, 2.5 * k, amber);
        polyline(
            &f.pts(&[(-17.0, -7.0), (-17.0, 7.0)]),
            false,
            2.5 * k,
            amber,
        );
    } else {
        polyline(&f.arc(-16.0, 0.0, 5.0, 0.0, TAU, 20), true, 1.5 * k, amber);
    }
    // Drone bay: a small hangar on the stern with a light per sentry aboard.
    if m.drone_bay {
        let bay = f.rect(-15.0, -6.0, 9.0, 12.0);
        fill_fan((bay[0] + bay[2]) / 2.0, &bay, Color::from_hex(0x1a2a30));
        polyline(&bay, true, 1.2 * k, Color::from_hex(col::CABLE));
        for i in 0..SENTRY_MAX {
            let lit = i < st.sentries;
            let c = Color::from_hex(if lit { col::CABLE } else { 0x14161c });
            fill_rect(f, -13.0, -4.0 + i as f32 * 3.2, 5.0, 2.0, c);
        }
    }
    // Missile pods: two racks on the flanks; the four tubes show how full
    // the rack is, whatever its size.
    if m.missiles {
        let rack = m.missile_rack.max(1);
        let full = f64::from(st.missiles.unwrap_or(rack)) / f64::from(rack);
        let loaded = cj_sim::js::round(full * 4.0) as u32;
        for side in [-1.0f32, 1.0] {
            let pod = f.rect(-2.0, side * 9.0 - 2.5, 12.0, 5.0);
            fill_fan((pod[0] + pod[2]) / 2.0, &pod, Color::from_hex(0x2b2f38));
            polyline(&pod, true, 1.2 * k, amber);
            for i in 0..2u32 {
                let slot = if side < 0.0 { 0 } else { 2 } + i;
                let c = Color::from_hex(if slot < loaded { col::DANGER } else { 0x14161c });
                fill_rect(f, 1.0 + i as f32 * 5.0, side * 9.0 - 1.2, 3.0, 2.4, c);
            }
        }
    }
    // Salvage grinder: a spinning toothed wheel on the port quarter.
    if m.grinder {
        let c = f.p(-11.0, -16.0);
        let spin = t * 8.0;
        let teeth: Vec<Vec2> = (0..16)
            .map(|i| {
                let r = if i % 2 == 1 { 4.0 } else { 7.0 } * k;
                let a = i as f32 / 16.0 * TAU + spin + f.s.atan2(f.c);
                c + vec2(a.cos(), a.sin()) * r
            })
            .collect();
        fill_fan(c, &teeth, Color::from_hex(0x2b2f38));
        polyline(&teeth, true, 1.2 * k, Color::from_hex(col::STEEL));
    }
}

/// Shield bubble: faint when full and resting, brighter as it takes hits,
/// gone when depleted; a rippling dashed ring while it recharges.
pub fn draw_shield(x: f32, y: f32, shield: f64, max: f64, flash: f64, delay: f64, t: f32) {
    if shield <= 0.0 {
        return;
    }
    let frac = (shield / max) as f32;
    let flash = flash as f32;
    draw_circle(
        x,
        y,
        34.0,
        rgba(col::SHIELD, (0.05 + frac * 0.06 + flash * 1.2).min(1.0)),
    );
    let ring = circle_pts(vec2(x, y), 34.0, 64);
    let c = rgba(col::SHIELD, (0.25 + frac * 0.35 + flash * 2.0).min(1.0));
    if delay <= 0.0 && frac < 1.0 {
        crate::draw::dashed(&ring, true, 6.0, 6.0, -t * 30.0, 1.5, c);
    } else {
        polyline(&ring, true, 1.5, c);
    }
}
