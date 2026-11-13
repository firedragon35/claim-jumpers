//! The field, drawn the way render() in src/js/17-rendering.js draws it:
//! starfield, boundary, carrier, wrecks, rocks, cable, salvage, tug,
//! particles and popups, then screen-space vignette and flash.
//!
//! Canvas paths become triangles and line segments here. Where the canvas
//! does something macroquad has no direct equivalent for (gradient fills
//! clipped to a path), the closest cheap approximation is used and noted.

use crate::controls::Device;
use crate::fx::{Fx, Kind};
use crate::look::{col, rgba, text_centered, Look};
use crate::settings::{key_label, Bind, Settings};
use crate::tug;
use cj_sim::consts::*;
use cj_sim::entities::{Crab, Item, Jumper, RadCloud, Rock, ShapePt, Wreck};
use cj_sim::items::ItemType;
use cj_sim::rng::Rng;
use cj_sim::World;
use macroquad::prelude::*;
use std::f32::consts::TAU;

/* ------------------------------ Helpers ------------------------------ */

pub(crate) fn v(x: f64, y: f64) -> Vec2 {
    vec2(x as f32, y as f32)
}

/// A rigid transform: local tug/rock coordinates to world coordinates.
#[derive(Clone, Copy)]
pub(crate) struct Xf {
    pub(crate) o: Vec2,
    c: f32,
    s: f32,
    /// Uniform scale of the local frame (1 except for boss hulls).
    k: f32,
}

impl Xf {
    pub(crate) fn new(x: f64, y: f64, angle: f64) -> Xf {
        Xf::scaled(x, y, angle, 1.0)
    }
    pub(crate) fn scaled(x: f64, y: f64, angle: f64, k: f32) -> Xf {
        Xf {
            o: v(x, y),
            c: angle.cos() as f32,
            s: angle.sin() as f32,
            k,
        }
    }
    pub(crate) fn p(&self, x: f32, y: f32) -> Vec2 {
        let (x, y) = (x * self.k, y * self.k);
        self.o + vec2(x * self.c - y * self.s, x * self.s + y * self.c)
    }
    pub(crate) fn pts(&self, local: &[(f32, f32)]) -> Vec<Vec2> {
        local.iter().map(|&(x, y)| self.p(x, y)).collect()
    }
}

/// A line through `pts`, optionally closed. Opaque lines get round joints
/// so corners don't show notches; translucent ones skip them to avoid
/// doubled alpha where segments meet.
pub(crate) fn polyline(pts: &[Vec2], closed: bool, width: f32, color: Color) {
    let n = pts.len();
    if n < 2 {
        return;
    }
    let segs = if closed { n } else { n - 1 };
    for i in 0..segs {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        draw_line(a.x, a.y, b.x, b.y, width, color);
    }
    if color.a >= 0.999 && width > 1.5 {
        for p in pts {
            draw_circle(p.x, p.y, width / 2.0, color);
        }
    }
}

/// glowStroke: the path stroked wide and faint, then thin and bright.
pub(crate) fn glow_stroke(pts: &[Vec2], closed: bool, rgb: u32, width: f32, alpha: f32) {
    polyline(pts, closed, width * 3.5, rgba(rgb, alpha * 0.22));
    polyline(pts, closed, width, rgba(rgb, alpha));
}

/// Fill a star-shaped polygon as a fan around `centre`.
pub(crate) fn fill_fan(centre: Vec2, pts: &[Vec2], color: Color) {
    for i in 0..pts.len() {
        draw_triangle(centre, pts[i], pts[(i + 1) % pts.len()], color);
    }
}

/// Fill any simple polygon by ear clipping (the carrier hull is concave).
pub(crate) fn fill_poly(pts: &[Vec2], color: Color) {
    let mut idx: Vec<usize> = (0..pts.len()).collect();
    // Winding sign, so "convex corner" means the same thing either way round.
    let area: f32 = (0..pts.len())
        .map(|i| {
            let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
            a.x * b.y - b.x * a.y
        })
        .sum();
    let sign = area.signum();
    let cross = |a: Vec2, b: Vec2, c: Vec2| (b - a).perp_dot(c - a);
    let inside = |p: Vec2, a: Vec2, b: Vec2, c: Vec2| {
        let (d1, d2, d3) = (
            cross(a, b, p) * sign,
            cross(b, c, p) * sign,
            cross(c, a, p) * sign,
        );
        d1 > 0.0 && d2 > 0.0 && d3 > 0.0
    };
    let mut guard = 0;
    while idx.len() > 3 && guard < 10_000 {
        guard += 1;
        let n = idx.len();
        let mut clipped = false;
        for i in 0..n {
            let (ia, ib, ic) = (idx[(i + n - 1) % n], idx[i], idx[(i + 1) % n]);
            let (a, b, c) = (pts[ia], pts[ib], pts[ic]);
            if cross(a, b, c) * sign <= 0.0 {
                continue; // reflex corner
            }
            if idx
                .iter()
                .any(|&j| j != ia && j != ib && j != ic && inside(pts[j], a, b, c))
            {
                continue;
            }
            draw_triangle(a, b, c, color);
            idx.remove(i);
            clipped = true;
            break;
        }
        if !clipped {
            break; // degenerate outline: give up rather than loop
        }
    }
    if idx.len() == 3 {
        draw_triangle(pts[idx[0]], pts[idx[1]], pts[idx[2]], color);
    }
}

/// A dashed line along a polyline, like setLineDash with lineDashOffset.
pub(crate) fn dashed(
    pts: &[Vec2],
    closed: bool,
    dash: f32,
    gap: f32,
    offset: f32,
    width: f32,
    color: Color,
) {
    let period = dash + gap;
    let mut along = offset.rem_euclid(period); // distance into the pattern
    let n = pts.len();
    let segs = if closed { n } else { n - 1 };
    for i in 0..segs {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        let len = a.distance(b);
        if len == 0.0 {
            continue;
        }
        let dir = (b - a) / len;
        // Dashes start at `pos` and every period after; each pass moves a
        // whole period, so float rounding on long edges can't stall it.
        let mut pos = -along;
        while pos < len {
            let (s, e) = (pos.max(0.0), (pos + dash).min(len));
            if e > s {
                let (p, q) = (a + dir * s, a + dir * e);
                draw_line(p.x, p.y, q.x, q.y, width, color);
            }
            pos += period;
        }
        along = (along + len).rem_euclid(period);
    }
}

pub(crate) fn circle_pts(c: Vec2, r: f32, n: usize) -> Vec<Vec2> {
    (0..n)
        .map(|i| {
            let a = i as f32 / n as f32 * TAU;
            c + vec2(a.cos(), a.sin()) * r
        })
        .collect()
}

/// A rock or scrap outline: radius multipliers around a circle (polyPath).
fn shape_pts(xf: &Xf, shape: &[ShapePt], r: f64) -> Vec<Vec2> {
    shape
        .iter()
        .map(|p| xf.p((p.a.cos() * r * p.m) as f32, (p.a.sin() * r * p.m) as f32))
        .collect()
}

/* ------------------------------ Starfield ------------------------------ */

struct Star {
    x: f32,
    y: f32,
    rgb: u32,
    tw: f32,
    ph: f32,
    bright: bool,
}

struct Layer {
    f: f32,
    size: f32,
    alpha: f32,
    stars: Vec<Star>,
}

/// Three parallax layers of twinkling stars over slow nebula clouds.
pub struct Stars {
    layers: Vec<Layer>,
}

const STAR_TINTS: [u32; 6] = [0xdde5f3, 0xdde5f3, 0xdde5f3, 0xbcd4ff, 0xffe2b8, 0xffd0e4];
const NEBULAE: [(f32, f32, f32, u32); 4] = [
    (0.2, 0.25, 0.75, 0x2a3a7a),
    (0.75, 0.65, 0.9, 0x4a2463),
    (0.45, 0.9, 0.6, 0x14505a),
    (0.95, 0.15, 0.5, 0x5a2a3a),
];

impl Stars {
    pub fn new(rng: &mut Rng) -> Stars {
        let layers = [0.08f32, 0.2, 0.45]
            .iter()
            .enumerate()
            .map(|(li, &f)| Layer {
                f,
                size: 0.7 + li as f32 * 0.55,
                alpha: 0.35 + li as f32 * 0.25,
                stars: (0..130 - li * 30)
                    .map(|_| Star {
                        x: rng.next() as f32,
                        y: rng.next() as f32,
                        rgb: STAR_TINTS[rng.index(STAR_TINTS.len())],
                        tw: rng.range(0.6, 2.2) as f32,
                        ph: rng.range(0.0, std::f64::consts::TAU) as f32,
                        bright: li == 2 && rng.chance(0.12),
                    })
                    .collect(),
            })
            .collect();
        Stars { layers }
    }

    /// Draw in screen space (logical pixels). `nebulae` draws the sprite
    /// nebulae; it's off when the effects' shader nebula took their place.
    fn draw(&self, look: &Look, cam: Vec2, t: f32, w: f32, h: f32, nebulae: bool) {
        let span = w.max(h);
        if nebulae {
            look.additive(|| {
                for &(nx, ny, nr, rgb) in &NEBULAE {
                    let x = (nx * w * 1.6 - cam.x * 0.02).rem_euclid(w * 1.6) - w * 0.3;
                    let y = (ny * h * 1.6 - cam.y * 0.02).rem_euclid(h * 1.6) - h * 0.3;
                    look.glow(x, y, nr * span, rgb, false, 0.22);
                }
            });
        }
        for l in &self.layers {
            for s in &l.stars {
                let x = (s.x * w * 3.0 - cam.x * l.f).rem_euclid(w);
                let y = (s.y * h * 3.0 - cam.y * l.f).rem_euclid(h);
                let a = l.alpha * (0.65 + 0.35 * (t * s.tw + s.ph).sin());
                draw_rectangle(x, y, l.size, l.size, rgba(s.rgb, a));
                if s.bright {
                    look.additive(|| {
                        look.glow(x + l.size / 2.0, y + l.size / 2.0, 5.0, s.rgb, true, a)
                    });
                }
            }
        }
    }
}

/* ------------------------------ The field ------------------------------ */

/// The field's dashed edge, from `lo` to `hi` on both axes.
fn draw_boundary(t: f32, lo: f64, hi: f64) {
    let (lo, hi) = (lo as f32, hi as f32);
    let pts = [vec2(lo, lo), vec2(hi, lo), vec2(hi, hi), vec2(lo, hi)];
    dashed(
        &pts,
        true,
        40.0,
        40.0,
        t * 20.0,
        6.0,
        rgba(col::HAZARD, 0.35),
    );
}

const CARRIER_HULL_PTS: [(f32, f32); 14] = [
    (-230.0, -70.0),
    (180.0, -80.0),
    (270.0, -40.0),
    (280.0, 40.0),
    (180.0, 80.0),
    (-230.0, 70.0),
    (-230.0, 95.0),
    (-440.0, 110.0),
    (-450.0, 80.0),
    (-250.0, 50.0),
    (-250.0, -50.0),
    (-450.0, -80.0),
    (-440.0, -110.0),
    (-230.0, -95.0),
];

fn draw_carrier(look: &Look, t: f32, scale: f32) {
    let xf = Xf::new(CARRIER_X, CARRIER_Y, 0.0);
    let hull = xf.pts(&CARRIER_HULL_PTS);
    fill_poly(&hull, crate::look::hex(0x121722));
    glow_stroke(&hull, true, col::HAZARD, 2.0, 1.0);
    // Panel lines.
    let mut x = -180.0;
    while x <= 160.0 {
        let (a, b) = (xf.p(x, -72.0), xf.p(x, 72.0));
        draw_line(a.x, a.y, b.x, b.y, 1.0, rgba(col::HAZARD, 0.25));
        x += 60.0;
    }
    // Stencilled name.
    let c = xf.p(30.0, 0.0);
    text_centered(
        "HOLLIS-3",
        c.x,
        c.y,
        46.0,
        &look.fonts.display,
        rgba(col::HAZARD, 0.8),
        scale,
    );
    let c = xf.p(30.0, 36.0);
    text_centered(
        "SALVAGE CARRIER · BAY ←",
        c.x,
        c.y,
        14.0,
        &look.fonts.mono,
        rgba(col::HAZARD, 0.8),
        scale,
    );
    // Blinking running lights on the bay arms.
    let on = (t * 4.0).sin() > 0.0;
    for (lx, ly) in [(-440.0, -95.0), (-440.0, 95.0), (270.0, 0.0)] {
        let p = xf.p(lx, ly);
        draw_circle(
            p.x,
            p.y,
            6.0,
            if on {
                crate::look::hex(col::DANGER)
            } else {
                crate::look::hex(0x3a1410)
            },
        );
    }

    // Bay intake ring.
    let bay = v(BAY_X, BAY_Y);
    let ring = circle_pts(bay, BAY_R as f32, 96);
    let a = 0.5 + (t * 3.0).sin() * 0.2;
    dashed(&ring, true, 14.0, 10.0, t * 30.0, 3.0, rgba(col::HAZARD, a));
    draw_circle(bay.x, bay.y, BAY_R as f32, rgba(col::HAZARD, 0.07));
    text_centered(
        "BAY",
        bay.x,
        bay.y,
        30.0,
        &look.fonts.display,
        rgba(col::HAZARD, 0.8),
        scale,
    );
}

fn draw_wreck(wr: &Wreck, t: f32) {
    let xf = Xf::new(wr.b.x, wr.b.y, wr.angle);
    let pts: Vec<Vec2> = wr
        .pts
        .iter()
        .map(|&(x, y)| xf.p(x as f32, y as f32))
        .collect();
    let flash = wr.flash > 0.0;
    fill_fan(
        xf.o,
        &pts,
        crate::look::hex(if flash { 0x1c3337 } else { col::WRECK_FILL }),
    );
    polyline(
        &pts,
        true,
        2.5,
        crate::look::hex(if flash { col::TEXT } else { col::WRECK }),
    );
    // Ribs: broken frame lines across the hull.
    let r = wr.b.r as f32;
    for i in -2..=2 {
        let i = i as f32;
        let (a, b) = (
            xf.p(i * r * 0.35, -r * 0.45),
            xf.p(i * r * 0.35 + 8.0, r * 0.45),
        );
        draw_line(
            a.x,
            a.y,
            b.x,
            b.y,
            1.5,
            Color::new(111.0 / 255.0, 163.0 / 255.0, 160.0 / 255.0, 0.4),
        );
    }
    // Cracks deepen with damage.
    let dmg = (1.0 - wr.hp / wr.max_hp) as f32;
    if dmg > 0.2 {
        let pts = xf.pts(&[
            (-r * 0.6, 0.0),
            (-r * 0.2, r * 0.15),
            (r * 0.1, -r * 0.1),
            (r * 0.2 + dmg * r * 0.5, r * 0.12),
        ]);
        polyline(&pts, false, 1.5, rgba(col::HAZARD, dmg.min(1.0)));
    }
    // A dying emergency light.
    if (t * 2.0 + wr.light as f32).sin() > 0.6 {
        let p = xf.p(r * 0.9, 0.0);
        draw_circle(p.x, p.y, 4.0, crate::look::hex(col::DANGER));
    }
}

/// Rock: dark body, a soft highlight to the upper left that stays put while
/// it spins, shadowed craters, and the outline.
fn draw_rock(look: &Look, rk: &Rock) {
    let xf = Xf::new(rk.b.x, rk.b.y, rk.angle);
    let r = rk.b.r as f32;
    let pts = shape_pts(&xf, &rk.shape, rk.b.r);
    let flash = rk.flash > 0.0;
    fill_fan(
        xf.o,
        &pts,
        crate::look::hex(if flash { 0x2b3448 } else { col::ROCK_FILL }),
    );
    // The browser fills a radial gradient clipped to the outline. Without
    // clipping, a smaller soft highlight keeps the light inside the rock.
    look.glow(
        xf.o.x - r * 0.35,
        xf.o.y - r * 0.35,
        r * 0.75,
        0xaab9d2,
        false,
        0.28,
    );
    for c in &rk.craters {
        let p = xf.p((c.a.cos() * c.d) as f32 * r, (c.a.sin() * c.d) as f32 * r);
        let cr = c.s as f32 * r;
        draw_circle(p.x, p.y, cr, Color::new(0.0, 0.0, 0.0, 0.28));
        draw_circle_lines(
            p.x,
            p.y,
            cr,
            1.0,
            Color::new(200.0 / 255.0, 215.0 / 255.0, 235.0 / 255.0, 0.08),
        );
    }
    polyline(
        &pts,
        true,
        2.0,
        crate::look::hex(if flash { col::TEXT } else { col::ROCK }),
    );
    if rk.veined {
        // Gold seams mark ore-bearing rock.
        let c = rgba(col::ORE, 0.85);
        polyline(
            &xf.pts(&[
                (-r * 0.5, -r * 0.2),
                (-r * 0.1, r * 0.1),
                (r * 0.35, -r * 0.05),
            ]),
            false,
            2.0,
            c,
        );
        polyline(
            &xf.pts(&[(-r * 0.2, r * 0.4), (r * 0.15, r * 0.3)]),
            false,
            2.0,
            c,
        );
    }
}

/// Fill a convex closed path (a rectangle, a box) as a triangle fan.
pub(crate) fn fill_convex(pts: &[Vec2], color: Color) {
    let c = pts.iter().fold(Vec2::ZERO, |a, p| a + *p) / pts.len() as f32;
    fill_fan(c, pts, color);
}

/// An axis-aligned local rectangle, as a closed outline.
pub(crate) fn rect_pts(xf: &Xf, x: f32, y: f32, w: f32, h: f32) -> Vec<Vec2> {
    xf.pts(&[(x, y), (x + w, y), (x + w, y + h), (x, y + h)])
}

/// Points along a local arc from `a0` to `a1` (radians).
pub(crate) fn arc_pts(xf: &Xf, cx: f32, cy: f32, r: f32, a0: f32, a1: f32, n: usize) -> Vec<Vec2> {
    (0..=n)
        .map(|k| {
            let a = a0 + (a1 - a0) * k as f32 / n as f32;
            xf.p(cx + a.cos() * r, cy + a.sin() * r)
        })
        .collect()
}

fn draw_item(look: &Look, it: &Item, t: f32, scale: f32) {
    let xf = Xf::new(it.b.x, it.b.y, it.angle);
    let r = it.b.r as f32;
    match it.kind {
        ItemType::Scrap => {
            let pts = shape_pts(&xf, &it.shape, it.b.r);
            fill_fan(xf.o, &pts, crate::look::hex(0x1a2130));
            if it.rare {
                glow_stroke(&pts, true, it.tint.unwrap_or(col::STEEL), 2.0, 1.0);
            } else {
                polyline(
                    &pts,
                    true,
                    2.0,
                    rgba(col::STEEL, if it.grade == 0 { 0.6 } else { 1.0 }),
                );
            }
        }
        ItemType::Ore => {
            // Each ore grade is its own colour: iron, copper, cobalt, platinum.
            let c = it.tint.unwrap_or(col::ORE);
            let pts = xf.pts(&[(0.0, -r), (r * 0.7, 0.0), (0.0, r), (-r * 0.7, 0.0)]);
            fill_fan(xf.o, &pts, rgba(c, 0.25));
            glow_stroke(&pts, true, c, 2.0, 1.0);
        }
        ItemType::Plate => {
            let c = it.tint.unwrap_or(col::STEEL);
            let body = rect_pts(&xf, -r, -r * 0.6, r * 2.0, r * 1.2);
            fill_convex(&body, crate::look::hex(0x18202e));
            if it.rare {
                glow_stroke(&body, true, c, 2.5, 1.0);
            } else {
                polyline(&body, true, 2.0, crate::look::hex(c));
            }
            // Rivets.
            for (x, y) in [(-0.75, -0.4), (0.75, -0.4), (-0.75, 0.4), (0.75, 0.4)] {
                let rv = rect_pts(&xf, x * r - 1.5, y * r - 1.5, 3.0, 3.0);
                fill_convex(&rv, crate::look::hex(c));
            }
            if it.grade == 0 {
                // A dent across the cheapest grade.
                let dent = xf.pts(&[(-r * 0.3, -r * 0.6), (0.0, 0.0), (r * 0.2, r * 0.6)]);
                polyline(&dent, false, 1.0, crate::look::hex(c));
            }
        }
        ItemType::Engine => {
            let c = it.tint.unwrap_or(col::ENGINE);
            let block = rect_pts(&xf, -r * 0.9, -r * 0.6, r * 1.3, r * 1.2);
            let nozzle = xf.pts(&[
                (r * 0.4, -r * 0.35),
                (r, -r * 0.55),
                (r, r * 0.55),
                (r * 0.4, r * 0.35),
            ]);
            fill_convex(&block, crate::look::hex(0x121b2c));
            fill_convex(&nozzle, crate::look::hex(0x121b2c));
            glow_stroke(&block, true, c, 2.5, 1.0);
            glow_stroke(&nozzle, false, c, 2.5, 1.0);
            for i in -2..=1 {
                let x = i as f32 * r * 0.25;
                let fin = xf.pts(&[(x, -r * 0.6), (x, r * 0.6)]);
                polyline(&fin, false, 1.0, rgba(c, 0.5));
            }
            if it.rare {
                // Fusion core glow.
                let p = xf.p(-r * 0.25, 0.0);
                draw_circle(p.x, p.y, r * 0.3, rgba(c, 0.35 + (t * 4.0).sin() * 0.15));
            }
        }
        ItemType::Reactor => {
            let c = it.tint.unwrap_or(col::REACTOR);
            // Cracked cores flicker faster.
            let rate = if it.grade == 0 { 11.0 } else { 5.0 };
            let pulse = 0.6 + (t * rate).sin() * 0.4;
            draw_circle(xf.o.x, xf.o.y, r, rgba(c, 0.12 + pulse * 0.15));
            glow_stroke(&circle_pts(xf.o, r, 32), true, c, 2.5, 1.0);
            // Trefoil blades, turning with the item.
            for i in 0..3 {
                let a = i as f32 / 3.0 * TAU;
                let mut blade = vec![xf.o];
                blade.extend(arc_pts(&xf, 0.0, 0.0, r * 0.7, a, a + 0.7, 6));
                fill_convex(&blade, crate::look::hex(c));
            }
            draw_circle(xf.o.x, xf.o.y, r * 0.18, crate::look::hex(col::VOID));
            // A red ring once it is primed or damaged.
            if it.fuse > 0.0 || it.hp.is_some_and(|hp| hp < 3.0) {
                draw_circle_lines(xf.o.x, xf.o.y, r + 6.0, 2.0, crate::look::hex(col::DANGER));
            }
        }
        ItemType::Blackbox => {
            // Flight recorder: a small hazard-orange box with a beacon light.
            let body = rect_pts(&xf, -10.0, -7.0, 20.0, 14.0);
            fill_convex(&body, crate::look::hex(0x3a2208));
            glow_stroke(&body, true, col::HAZARD, 2.0, 1.0);
            for x in [-6.0, 2.0] {
                fill_convex(
                    &rect_pts(&xf, x, -7.0, 2.5, 14.0),
                    crate::look::hex(col::HAZARD),
                );
            }
            let beacon = xf.p(8.0, -9.0);
            let lit = (t * 7.0).sin() > 0.3;
            draw_circle(
                beacon.x,
                beacon.y,
                2.5,
                crate::look::hex(if lit { col::DANGER } else { 0x401510 }),
            );
        }
        ItemType::Pod => {
            // Sealed cargo pod: a capsule stencilled with an upright "?".
            let half = std::f32::consts::FRAC_PI_2;
            let mut capsule = arc_pts(&xf, -r * 0.45, 0.0, r * 0.55, half, 3.0 * half, 10);
            capsule.extend(arc_pts(&xf, r * 0.45, 0.0, r * 0.55, -half, half, 10));
            fill_convex(&capsule, crate::look::hex(0x1c1a2c));
            glow_stroke(&capsule, true, col::COLLECTOR, 2.0, 1.0);
            text_centered(
                "?",
                xf.o.x,
                xf.o.y + 1.0,
                16.0,
                &look.fonts.display,
                crate::look::hex(col::TEXT),
                scale,
            );
        }
        ItemType::Charge => {
            let warn = it.fuse > 0.0 || it.hp.is_some_and(|hp| hp < 2.0);
            crate::foes::charge(&xf, t, warn);
        }
        ItemType::Ingot => {
            let c = it.tint.unwrap_or(col::ORE);
            let pts = xf.pts(&[
                (-r, r * 0.5),
                (-r * 0.6, -r * 0.5),
                (r * 0.6, -r * 0.5),
                (r, r * 0.5),
            ]);
            fill_convex(&pts, rgba(c, 0.35));
            glow_stroke(&pts, true, c, 2.5, 1.0);
            polyline(
                &xf.pts(&[(-r * 0.4, -r * 0.1), (r * 0.4, -r * 0.1)]),
                false,
                1.0,
                crate::look::hex(c),
            );
        }
        ItemType::Keycard => {
            // A small card with a magnetic stripe, blinking so it can be spotted.
            let body = rect_pts(&xf, -r, -r * 0.65, r * 2.0, r * 1.3);
            fill_convex(&body, crate::look::hex(0x0e2230));
            glow_stroke(&body, true, col::CABLE, 2.0, 1.0);
            fill_convex(
                &rect_pts(&xf, -r, -2.0, r * 2.0, 3.0),
                crate::look::hex(col::CABLE),
            );
            let p = xf.o;
            look.additive(|| {
                look.glow(
                    p.x,
                    p.y,
                    10.0 + 5.0 * (t * 4.0).sin().abs(),
                    col::CABLE,
                    true,
                    1.0,
                )
            });
        }
        ItemType::Core => {
            // Rogue core: a red hexagon with a pulsing, flickering heart.
            let hex: Vec<Vec2> = (0..6)
                .map(|i| {
                    let a = i as f32 / 6.0 * TAU;
                    xf.p(a.cos() * r, a.sin() * r)
                })
                .collect();
            fill_fan(xf.o, &hex, crate::look::hex(0x2a0b0b));
            glow_stroke(&hex, true, col::DANGER, 2.0, 1.0);
            if (t * 19.0 + it.b.x as f32).sin() > -0.5 {
                let p = xf.o;
                look.additive(|| {
                    look.glow(
                        p.x,
                        p.y,
                        9.0 + (t * 7.0).sin() * 3.0,
                        col::DANGER,
                        true,
                        1.0,
                    )
                });
            }
        }
    }
}

/// A rival: its tow line and hull, plus the sever countdown ring while it
/// is latched onto your cargo.
fn draw_jumper(look: &Look, w: &World, j: &Jumper) {
    let hooked = j.hook.and_then(|id| w.items.iter().find(|it| it.id == id));
    if let Some(it) = hooked {
        let pts = [v(j.b.x, j.b.y), v(it.b.x, it.b.y)];
        dashed(
            &pts,
            false,
            6.0,
            6.0,
            0.0,
            1.5 * 3.5,
            rgba(col::JUMPER, 0.22),
        );
        dashed(
            &pts,
            false,
            6.0,
            6.0,
            0.0,
            1.5,
            crate::look::hex(col::JUMPER),
        );
    }
    let k = if j.boss.is_some() { 1.4 } else { 1.0 };
    let xf = Xf::scaled(j.b.x, j.b.y, j.angle, k);
    let def = j.variant.def();
    let edge = if j.flash > 0.0 {
        col::TEXT
    } else {
        def.stroke.unwrap_or(col::JUMPER)
    };
    let outline: &[(f32, f32)] = match j.variant {
        // Skiff: a thin dart, all engine.
        Variant::Skiff => &[(16.0, 0.0), (-10.0, -7.0), (-6.0, 0.0), (-10.0, 7.0)],
        // Brute: a blunt slab with a grab-claw prow.
        Variant::Brute => &[
            (20.0, -6.0),
            (24.0, 0.0),
            (20.0, 6.0),
            (10.0, 15.0),
            (-16.0, 15.0),
            (-20.0, 6.0),
            (-20.0, -6.0),
            (-16.0, -15.0),
            (10.0, -15.0),
        ],
        // Gunship: a swept wing with gun pods on the tips.
        Variant::Gunship => &[
            (18.0, 0.0),
            (0.0, -6.0),
            (-6.0, -17.0),
            (-14.0, -17.0),
            (-10.0, 0.0),
            (-14.0, 17.0),
            (-6.0, 17.0),
            (0.0, 6.0),
        ],
        Variant::Cutter => &[(18.0, 0.0), (-11.0, -12.0), (-5.0, 0.0), (-11.0, 12.0)],
    };
    let pts = xf.pts(outline);
    fill_poly(&pts, crate::look::hex(def.hull));
    glow_stroke(&pts, true, edge, 2.0, 1.0);
    match j.variant {
        Variant::Gunship => {
            fill_convex(
                &rect_pts(&xf, -8.0, -18.0, 10.0, 3.0),
                crate::look::hex(edge),
            );
            fill_convex(
                &rect_pts(&xf, -8.0, 15.0, 10.0, 3.0),
                crate::look::hex(edge),
            );
        }
        Variant::Brute => fill_convex(
            &rect_pts(&xf, -12.0, -3.0, 18.0, 6.0),
            crate::look::hex(edge),
        ),
        _ => {}
    }
    if j.hunter {
        // Hunters wear red war paint and run hot: they are here for you.
        let red = crate::look::hex(col::DANGER);
        polyline(&xf.pts(&[(6.0, -4.0), (-6.0, -8.0)]), false, 2.0, red);
        polyline(&xf.pts(&[(6.0, 4.0), (-6.0, 8.0)]), false, 2.0, red);
        let p = xf.p(-8.0, 0.0);
        look.additive(|| look.glow(p.x, p.y, 14.0, col::DANGER, true, 1.0));
    }
    // Sever countdown ring while latched to your cargo.
    if let Some(it) = hooked.filter(|it| it.towed && j.latch_t > 0.0) {
        let frac = (j.latch_t / (JUMPER_SEVER_TIME * def.sever)) as f32;
        let top = -std::f32::consts::FRAC_PI_2;
        let at = Xf::new(it.b.x, it.b.y, 0.0);
        let ring = arc_pts(
            &at,
            0.0,
            0.0,
            it.b.r as f32 + 10.0,
            top,
            top + TAU * frac,
            48,
        );
        polyline(&ring, false, 3.0, crate::look::hex(col::JUMPER));
    }
}

fn draw_crab(c: &Crab) {
    let o = v(c.x, c.y);
    let leg = crate::look::hex(col::CRAB);
    for i in 0..6 {
        let a = i as f32 / 6.0 * TAU + (c.leg_t as f32 + i as f32).sin() * 0.3;
        let e = o + vec2(a.cos(), a.sin()) * 13.0;
        draw_line(o.x, o.y, e.x, e.y, 1.5, leg);
    }
    draw_ellipse(o.x, o.y, 7.0, 5.5, 0.0, leg);
    let eye = crate::look::hex(col::VOID);
    draw_rectangle(o.x - 3.0, o.y - 2.0, 2.0, 2.0, eye);
    draw_rectangle(o.x + 1.0, o.y - 2.0, 2.0, 2.0, eye);
}

/// Your rounds as hot amber streaks, and rival bolts as pink glows, all
/// in additive light.
fn draw_shots(look: &Look, w: &World) {
    look.additive(|| {
        for b in &w.bullets {
            let len = 0.018;
            let pts = [v(b.x - b.vx * len, b.y - b.vy * len), v(b.x, b.y)];
            glow_stroke(&pts, false, col::HAZARD, 2.2, 1.0);
            look.glow(b.x as f32, b.y as f32, 7.0, col::HAZARD, true, 1.0);
        }
        // SWAT rounds and stun bolts are bigger; SWAT, stun and rogue bolts
        // each read by colour.
        for b in &w.ebullets {
            let size = if b.swat {
                15.0
            } else if b.stun {
                13.0
            } else {
                11.0
            };
            let colour = if b.swat || b.rogue {
                col::DANGER
            } else if b.stun {
                col::LAW
            } else {
                col::JUMPER
            };
            look.glow(b.x as f32, b.y as f32, size, colour, true, 1.0);
        }
    });
}

/// Pilot `p`'s tow line, from its tug's stern down its chain. A taut link
/// is straight and brightens with tension; a slack link sags into a curve
/// so the rope reads as rope.
fn draw_cable(w: &World, p: usize) {
    let s = &w.pilots[p].ship;
    if !s.alive || w.pilots[p].chain.is_empty() {
        return;
    }
    let mut a = {
        let (x, y) = s.stern();
        v(x, y)
    };
    for (i, id) in w.pilots[p].chain.iter().enumerate() {
        let Some(it) = w.items.iter().find(|it| it.id == *id) else {
            continue;
        };
        let b = v(it.b.x, it.b.y);
        let d = a.distance(b).max(1.0);
        let link = it.link_len as f32;
        let pts: Vec<Vec2> = if d < link * 0.97 {
            let side = if i % 2 == 1 { 1.0 } else { -1.0 };
            let sag = (link * link - d * d).max(0.0).sqrt() * 0.35 * side;
            let dd = b - a;
            let cp = (a + b) / 2.0 + vec2(-dd.y / d, dd.x / d) * sag;
            (0..=12)
                .map(|k| {
                    let u = k as f32 / 12.0;
                    a * (1.0 - u) * (1.0 - u) + cp * 2.0 * u * (1.0 - u) + b * u * u
                })
                .collect()
        } else {
            vec![a, b]
        };
        let tns = it.tension as f32;
        // (Every pilot's chain; rivals draw their line in draw_jumper.)
        glow_stroke(
            &pts,
            false,
            if tns > 0.6 { col::TEXT } else { col::CABLE },
            1.6 + tns * 1.4,
            1.0,
        );
        a = b;
    }
}

/// Pilot `p`'s tug (drawTug with its mounted parts) and shield.
fn draw_ship(look: &Look, w: &World, p: usize, t: f32, rng: &mut Rng, paint: Option<(u32, u32)>) {
    let s = &w.pilots[p].ship;
    if !s.alive || (s.invuln > 0.0 && ((t * 12.0).floor() as i64) % 2 == 1) {
        return; // respawn flicker
    }
    let f = tug::Frame::new(s.b.x as f32, s.b.y as f32, s.angle as f32, 1.0);
    let st = tug::TugState {
        t,
        thrust: s.thrust_level as f32,
        sputtering: s.sputtering,
        strafe: s.strafe as f32,
        flash: s.flash > 0.0,
        overheated: s.overheated,
        sentries: 0,
        burn: s.burn_t > 0.0,
        missiles: Some(s.missiles),
        paint,
    };
    tug::draw_tug(look, &f, &w.pilots[p].mods, &st, rng);
    tug::draw_shield(
        s.b.x as f32,
        s.b.y as f32,
        s.shield,
        w.pilots[p].mods.shield_max,
        s.shield_flash,
        s.shield_delay_t,
        t,
    );
}

/// `${it.mass}` in JavaScript: no decimals for whole numbers.
fn js_num(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{v:.0}")
    } else {
        format!("{v}")
    }
}

/// The dashed ring and label over the piece the claw would grab.
/// A radiation cloud: a green wash and a marching edge, fading as it ends.
fn draw_radiation(look: &Look, cl: &RadCloud, t: f32, scale: f32) {
    const GREEN: u32 = 0x7dff5a;
    let fade = (cl.t / 3.0).min(1.0) as f32;
    let (p, r) = (v(cl.x, cl.y), RAD_CLOUD_RADIUS as f32);
    draw_circle(p.x, p.y, r, rgba(GREEN, 0.10 * fade));
    dashed(
        &circle_pts(p, r, 72),
        true,
        12.0,
        10.0,
        t * 12.0,
        2.0,
        rgba(GREEN, 0.6 * fade),
    );
    crate::foes::tag(look, p.x, p.y - r - 14.0, "RADIATION", GREEN, scale);
}

fn draw_hook_target(
    look: &Look,
    w: &World,
    t: f32,
    device: Device,
    settings: &Settings,
    scale: f32,
) {
    let Some(i) = w.find_hook_target(0) else {
        return;
    };
    let it = &w.items[i];
    let too_heavy = w.tow_mass(0) + it.b.mass > w.me().mods.winch_cap;
    let c = if too_heavy { col::DANGER } else { col::CABLE };
    let p = v(it.b.x, it.b.y);
    let r = it.b.r as f32;
    dashed(
        &circle_pts(p, r + 12.0, 48),
        true,
        6.0,
        6.0,
        t * 30.0,
        2.0,
        crate::look::hex(c),
    );
    let key = if device == Device::Pad {
        "Y".to_owned()
    } else {
        key_label(&settings.codes_for(Bind::Hook)[0])
    };
    let line1 = if too_heavy {
        "TOO HEAVY".to_owned()
    } else {
        format!("{key} · {}", it.label().to_uppercase())
    };
    text_centered(
        &line1,
        p.x,
        p.y - r - 34.0,
        13.0,
        &look.fonts.mono,
        crate::look::hex(c),
        scale,
    );
    let worth = if it.hidden {
        "??? cr".to_owned()
    } else if it.value > 0.0 {
        format!("{} cr", cj_sim::world::fmt_credits(it.value))
    } else {
        "no resale".to_owned()
    };
    let line2 = format!(
        "{worth}{} · {} t",
        if it.rare { " · RARE" } else { "" },
        js_num(it.b.mass)
    );
    let c2 = if it.rare {
        it.tint.unwrap_or(col::TEXT)
    } else {
        col::TEXT
    };
    text_centered(
        &line2,
        p.x,
        p.y - r - 20.0,
        13.0,
        &look.fonts.mono,
        crate::look::hex(c2),
        scale,
    );
}

/// Particles: smoke in normal blending, then `between` (the shots), then
/// everything that glows in additive light, in the browser's order.
fn draw_particles(
    look: &Look,
    fx: &Fx,
    visible: impl Fn(f64, f64, f64) -> bool,
    between: impl FnOnce(),
) {
    // Smoke first, in normal blending, so the glowing fire draws over it.
    for p in &fx.particles {
        if p.kind != Kind::Smoke || !visible(p.x, p.y, 0.0) {
            continue;
        }
        let f = (p.life / p.max) as f32;
        let r = p.size as f32 * (1.0 + (1.0 - f) * p.grow as f32);
        draw_circle(p.x as f32, p.y as f32, r, rgba(p.rgb, 0.5 * f));
    }
    between();
    // Everything that glows adds light, so overlaps brighten toward white.
    look.additive(|| {
        for p in &fx.particles {
            if p.kind == Kind::Smoke || !visible(p.x, p.y, p.ring) {
                continue;
            }
            let f = (p.life / p.max) as f32;
            let (x, y, size) = (p.x as f32, p.y as f32, p.size as f32);
            match p.kind {
                Kind::Ring => {
                    // Shockwave: races out fast then slows, thinning as it fades.
                    let r = p.ring as f32 * (1.0 - 0.85 * f * f);
                    glow_stroke(
                        &circle_pts(vec2(x, y), r, 64),
                        true,
                        p.rgb,
                        (size * f).max(0.5),
                        f,
                    );
                }
                Kind::Flash => look.glow(
                    x,
                    y,
                    size * (0.6 + (1.0 - f) * p.grow as f32),
                    p.rgb,
                    true,
                    f * f,
                ),
                Kind::Shard => {
                    let (bx, by) = (x - p.vx as f32 * 0.04, y - p.vy as f32 * 0.04);
                    draw_line(x, y, bx, by, size, rgba(p.rgb, f));
                }
                _ => look.glow(x, y, size * 2.2, p.rgb, true, f),
            }
        }
    });
}

/// Screen-edge darkening, rebuilt when the window size changes.
pub struct Vignette {
    size: (u32, u32),
    tex: Option<Texture2D>,
}

impl Vignette {
    pub fn new() -> Vignette {
        Vignette {
            size: (0, 0),
            tex: None,
        }
    }

    fn draw(&mut self, w: f32, h: f32) {
        // A quarter-resolution texture, stretched: the gradient is smooth.
        let size = (((w / 4.0) as u32).max(1), ((h / 4.0) as u32).max(1));
        if self.size != size || self.tex.is_none() {
            let (tw, th) = size;
            let mut img =
                Image::gen_image_color(tw as u16, th as u16, Color::new(0.0, 0.0, 0.0, 0.0));
            let (cx, cy) = (tw as f32 / 2.0, th as f32 / 2.0);
            let r = (cx * cx + cy * cy).sqrt();
            for y in 0..th {
                for x in 0..tw {
                    let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
                    let k = ((d - r * 0.45) / (r * 0.55)).clamp(0.0, 1.0);
                    img.set_pixel(x, y, Color::new(0.0, 0.0, 0.0, 0.6 * k));
                }
            }
            let tex = Texture2D::from_image(&img);
            tex.set_filter(FilterMode::Linear);
            self.tex = Some(tex);
            self.size = size;
        }
        if let Some(tex) = &self.tex {
            draw_texture_ex(
                tex,
                0.0,
                0.0,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(w, h)),
                    ..Default::default()
                },
            );
        }
    }
}

/// Camera that shows `r` on the window with Y pointing down, like the
/// browser canvas. macroquad 0.4's `from_display_rect` points Y up (it is
/// meant for render targets), which drew every screen upside down.
///
/// While the effects are drawing the field off-screen, the camera targets
/// that render target instead of the window (see `post`).
pub fn display_camera(r: Rect) -> Camera2D {
    let mut cam = Camera2D::from_display_rect(r);
    cam.zoom.y = -cam.zoom.y;
    cam.render_target = crate::post::target();
    cam
}

/// Logical-pixel camera over the whole window, for screen-space drawing.
pub fn screen_camera(view: (f32, f32)) {
    set_camera(&display_camera(Rect::new(0.0, 0.0, view.0, view.1)));
}

/// Everything the scene needs from the game layer.
pub struct Scene<'a> {
    pub look: &'a Look,
    pub world: &'a World,
    pub fx: &'a Fx,
    pub stars: &'a Stars,
    pub vignette: &'a mut Vignette,
    /// Visual-only randomness (flame flicker, shake).
    pub rng: &'a mut Rng,
    pub device: Device,
    /// For the key names in the net and hook prompts.
    pub settings: &'a Settings,
    /// Logical viewport size and the display's pixel ratio.
    pub view: (f32, f32),
    pub dpi: f32,
    /// The tug and its cable are drawn on every screen but the title.
    pub show_tug: bool,
    /// The hook target only shows while flying.
    pub playing: bool,
    /// The pilot's paint job, if one is chosen and unlocked.
    pub paint: Option<(u32, u32)>,
    /// Bloom, grade and nebula; `effects` is the player's setting.
    pub post: &'a mut crate::post::Post,
    pub effects: bool,
}

pub fn render(sc: Scene) {
    let w = sc.world;
    let t = w.t as f32;
    let (vw, vh) = sc.view;
    sc.post.begin(sc.effects, sc.view, sc.dpi);
    // The camera comes first so the clear lands on the effects' target
    // when they're on (clearing applies to the current camera's target).
    screen_camera(sc.view);
    clear_background(crate::look::hex(col::VOID));
    if let Some((rgb, a)) = w.belt.def().tint {
        draw_rectangle(0.0, 0.0, vw, vh, rgba(rgb, a));
    }
    let cam = v(w.cam.x, w.cam.y);
    let shader_nebula = sc.post.nebula(cam, t, sc.view);
    sc.stars.draw(sc.look, cam, t, vw, vh, !shader_nebula);

    // World camera, with screen shake.
    let z = w.cam.zoom as f32;
    // Reduced motion keeps a fifth of the shake (motionScale).
    let shake = w.shake * crate::look::motion_scale();
    let (sx, sy) = if shake > 0.0 {
        (
            sc.rng.range(-1.0, 1.0) * shake,
            sc.rng.range(-1.0, 1.0) * shake,
        )
    } else {
        (0.0, 0.0)
    };
    let centre = v(w.cam.x + sx, w.cam.y + sy);
    set_camera(&display_camera(Rect::new(
        centre.x - vw / 2.0 / z,
        centre.y - vh / 2.0 / z,
        vw / z,
        vh / z,
    )));
    // World text rasterizes at its on-screen pixel size.
    let scale = z * sc.dpi;

    // Cull anything outside the visible world rectangle (plus a margin).
    let (hw, hh) = (
        f64::from(vw / 2.0 / z) + 160.0,
        f64::from(vh / 2.0 / z) + 160.0,
    );
    let (cx, cy) = (w.cam.x, w.cam.y);
    let visible = |x: f64, y: f64, r: f64| (x - cx).abs() < hw + r && (y - cy).abs() < hh + r;

    draw_boundary(t, w.edge_lo, w.edge_hi);
    draw_carrier(sc.look, t, scale);
    // The refinery's pull field can be larger than the screen.
    if let Some(r) = &w.refinery {
        crate::foes::refinery(sc.look, r, t, scale);
    }
    for wr in &w.wrecks {
        if visible(wr.b.x, wr.b.y, wr.b.r) {
            draw_wreck(wr, t);
            let (x, y) = (wr.b.x as f32, (wr.b.y - wr.b.r * 0.8 - 14.0) as f32);
            if wr.freighter {
                crate::foes::tag(sc.look, x, y, "DERELICT FREIGHTER", col::ORE, scale);
            } else if wr.locked {
                let s = "SEALED HOLD · NEEDS KEYCARD";
                crate::foes::tag(sc.look, x, y, s, col::CABLE, scale);
            }
        }
    }
    for rk in &w.rocks {
        if visible(rk.b.x, rk.b.y, rk.b.r) {
            draw_rock(sc.look, rk);
        }
    }
    if sc.show_tug {
        for p in 0..w.pilots.len() {
            draw_cable(w, p);
        }
    }
    for it in &w.items {
        if visible(it.b.x, it.b.y, it.b.r) {
            // drawTrail: fading cable-coloured strokes, thicker toward the piece.
            if let Some(tr) = sc.fx.trails.get(&it.id).filter(|tr| tr.len() >= 2) {
                let n = tr.len() as f32;
                for i in 1..tr.len() {
                    let k = i as f32 / n;
                    let (a, b) = (tr[i - 1], tr[i]);
                    let wdt = it.b.r as f32 * k * 1.2;
                    let c = rgba(col::CABLE, k * 0.5);
                    draw_line(a.0 as f32, a.1 as f32, b.0 as f32, b.1 as f32, wdt, c);
                    draw_circle(b.0 as f32, b.1 as f32, wdt / 2.0, c);
                }
            }
            draw_item(sc.look, it, t, scale);
            if it.kind == ItemType::Keycard && !it.towed {
                let (x, y) = (it.b.x as f32, it.b.y as f32 - 22.0);
                crate::foes::tag(sc.look, x, y, "KEYCARD", col::CABLE, scale);
            }
        }
    }
    for cl in &w.clouds {
        if visible(cl.x, cl.y, RAD_CLOUD_RADIUS) {
            draw_radiation(sc.look, cl, t, scale);
        }
    }
    sc.look.additive(|| {
        for &(x0, y0, x1, y1, left) in &sc.fx.beams {
            let pts = [v(x0, y0), v(x1, y1)];
            let fade = (left / crate::fx::BEAM_TIME) as f32;
            glow_stroke(&pts, false, col::CABLE, 3.0, fade);
        }
    });
    for j in &w.jumpers {
        if visible(j.b.x, j.b.y, j.b.r) {
            draw_jumper(sc.look, w, j);
            if let Some(b) = &j.boss {
                let name = b.name.to_uppercase();
                let (x, y) = (j.b.x as f32, j.b.y as f32 - 38.0);
                crate::foes::tag(sc.look, x, y, &name, col::HAZARD, scale);
            }
        }
    }
    if let Some(h) = w.hauler.as_ref().filter(|h| !h.dead) {
        if visible(h.b.x, h.b.y, h.b.r) {
            crate::foes::hauler(sc.look, h, t, scale);
        }
    }
    crate::foes::draw_all(sc.look, w, t, sc.rng, scale, &visible);
    for c in &w.crabs {
        if visible(c.x, c.y, c.r) {
            draw_crab(c);
        }
    }
    if sc.show_tug {
        // Every pilot's tug: yours in your paint job, each other pilot in
        // its slot's colour.
        for p in 0..w.pilots.len() {
            let paint = if p == w.view_pilot {
                sc.paint
            } else {
                Some(crate::online::slot_paint(p))
            };
            draw_ship(sc.look, w, p, t, sc.rng, paint);
            draw_pvp_mark(sc.look, w, p, scale);
        }
        draw_partner(sc.look, w, t, sc.rng, scale);
        crate::foes::net_on_ship(sc.look, w, t, sc.device, sc.settings, scale);
        crate::foes::cuff_meter(sc.look, w, t, scale);
    }
    draw_particles(sc.look, sc.fx, visible, || draw_shots(sc.look, w));
    for p in &sc.fx.popups {
        let c = rgba(p.rgb, p.life.min(1.0) as f32);
        crate::look::text_centered_stars(
            &p.text,
            p.x as f32,
            p.y as f32,
            24.0,
            &sc.look.fonts.display,
            c,
            scale,
        );
    }
    if sc.playing && !w.shift_over {
        draw_hook_target(sc.look, w, t, sc.device, sc.settings, scale);
    }

    // Glow and grade the field, before the overlays and HUD so those stay
    // sharp. Reduced motion holds the grain still.
    let seed = if crate::look::reduce_motion() {
        0.0
    } else {
        (t * 60.0).floor() % 97.0
    };
    sc.post.finish(sc.view, seed);

    // Screen-space overlays.
    screen_camera(sc.view);
    if w.cond.def().night && sc.playing {
        draw_night(w, sc.view);
    }
    sc.vignette.draw(vw, vh);
    // Big nearby explosions wash the screen with light for a moment.
    if w.screen_flash > 0.0 {
        draw_rectangle(
            0.0,
            0.0,
            vw,
            vh,
            Color::new(
                1.0,
                244.0 / 255.0,
                220.0 / 255.0,
                (w.screen_flash * crate::look::motion_scale()) as f32,
            ),
        );
    }
    // The ion storm: a flickering violet wash and static streaks.
    if w.ion_t > 0.0 && sc.playing && !w.shift_over {
        let a = 0.07 + 0.04 * (t * 7.0).sin();
        draw_rectangle(
            0.0,
            0.0,
            vw,
            vh,
            Color::new(150.0 / 255.0, 90.0 / 255.0, 1.0, a),
        );
        let streak = Color::new(200.0 / 255.0, 170.0 / 255.0, 1.0, 0.35);
        let streaks = if crate::look::reduce_motion() { 0 } else { 4 };
        for _ in 0..streaks {
            let y = sc.rng.range(0.0, f64::from(vh)) as f32;
            let dy = sc.rng.range(-20.0, 20.0) as f32;
            draw_line(0.0, y, vw, y + dy, 1.0, streak);
        }
    }
    // Under attack: a red edge pulses around the screen while the warning holds.
    if w.me().under_attack_t > 0.0 && sc.playing && !w.shift_over {
        let pulse = if crate::look::reduce_motion() {
            0.18
        } else {
            0.3 * (0.5 + 0.5 * (t * 10.0).sin())
        };
        let a = pulse * (w.me().under_attack_t as f32).min(1.0);
        draw_rectangle_lines(
            0.0,
            0.0,
            vw,
            vh,
            12.0,
            Color::new(1.0, 90.0 / 255.0, 60.0 / 255.0, a),
        );
    }
    // A red wash for a moment after the hull takes a hit.
    if w.me().ship.flash > 0.0 && sc.playing && !w.shift_over {
        draw_rectangle(
            0.0,
            0.0,
            vw,
            vh,
            Color::new(1.0, 90.0 / 255.0, 60.0 / 255.0, 0.12),
        );
    }
}

/// drawNight: everything past the tug's lamp fades to near-black, as a
/// radial gradient from NIGHT_LIGHT (clear) to NIGHT_FADE (90% dark). The
/// gradient is a vertex-coloured ring mesh; beyond it a second ring out to
/// the screen corners stays fully dark.
fn draw_night(w: &World, view: (f32, f32)) {
    use cj_sim::field::{NIGHT_FADE, NIGHT_LIGHT};
    let (vw, vh) = view;
    let z = w.cam.zoom as f32;
    let (sx, sy) = if w.me().ship.alive {
        (w.me().ship.b.x, w.me().ship.b.y)
    } else {
        (BAY_X, BAY_Y)
    };
    let cx = vw / 2.0 + (sx - w.cam.x) as f32 * z;
    let cy = vh / 2.0 + (sy - w.cam.y) as f32 * z;
    let (r0, r1) = (NIGHT_LIGHT as f32 * z, NIGHT_FADE as f32 * z);
    // Far enough to cover the view from any lamp position on screen.
    let r2 = r1 + vw.hypot(vh) * 2.0;
    let clear = Color::new(2.0 / 255.0, 3.0 / 255.0, 8.0 / 255.0, 0.0);
    let dark = Color::new(2.0 / 255.0, 3.0 / 255.0, 8.0 / 255.0, 0.9);
    ring_mesh(cx, cy, r0, r1, clear, dark);
    ring_mesh(cx, cy, r1, r2, dark, dark);
}

/// A filled annulus with `inner` colour at radius `a` blending to `outer`
/// at radius `b`.
fn ring_mesh(cx: f32, cy: f32, a: f32, b: f32, inner: Color, outer: Color) {
    const N: usize = 64;
    let mut vertices = Vec::with_capacity(N * 2);
    let mut indices = Vec::with_capacity(N * 6);
    for i in 0..N {
        let ang = i as f32 / N as f32 * TAU;
        let (s, c) = ang.sin_cos();
        vertices.push(Vertex::new(cx + c * a, cy + s * a, 0.0, 0.0, 0.0, inner));
        vertices.push(Vertex::new(cx + c * b, cy + s * b, 0.0, 0.0, 0.0, outer));
        let (i0, i1) = ((i * 2) as u16, (i * 2 + 1) as u16);
        let (j0, j1) = ((((i + 1) % N) * 2) as u16, (((i + 1) % N) * 2 + 1) as u16);
        indices.extend_from_slice(&[i0, i1, j1, i0, j1, j0]);
    }
    draw_mesh(&Mesh {
        vertices,
        indices,
        texture: None,
    });
}

/// drawPartner: P2's tug, a Mk II with a cutter at 0.8 scale in its class
/// colours, a dashed line while latched to the cable's tail, and its tag.
fn draw_partner(look: &Look, w: &World, t: f32, rng: &mut Rng, scale: f32) {
    use cj_sim::parts::{Levels, Part};
    let Some(p) = &w.p2 else { return };
    let tail = w
        .me()
        .chain
        .last()
        .and_then(|id| w.items.iter().find(|it| it.id == *id));
    if let Some(it) = tail.filter(|_| p.linked) {
        let pts = [v(it.b.x, it.b.y), v(p.b.x, p.b.y)];
        dashed(
            &pts,
            false,
            5.0,
            5.0,
            0.0,
            1.5 * 3.5,
            rgba(col::CABLE, 0.22),
        );
        dashed(
            &pts,
            false,
            5.0,
            5.0,
            0.0,
            1.5,
            crate::look::hex(col::CABLE),
        );
    }
    let mods = cj_sim::mods::compute_mods(
        &[Some(Part::Cannon), None, None, None, None],
        &Levels::default(),
        1,
    );
    let f = tug::Frame::new(p.b.x as f32, p.b.y as f32, p.angle as f32, 0.8);
    let st = tug::TugState {
        t,
        thrust: p.thrust as f32,
        ..tug::TugState::default()
    };
    tug::draw_tug(look, &f, &mods, &st, rng);
    let label = if p.linked { "P2 · LATCHED" } else { "P2" };
    crate::foes::tag(
        look,
        p.b.x as f32,
        p.b.y as f32 - 26.0,
        label,
        col::CABLE,
        scale,
    );
}

/// A red ring and a "PVP" tag on pilot `p`'s tug while it is PvP Enabled,
/// so everyone can see who may open fire (your own tug included).
fn draw_pvp_mark(look: &crate::look::Look, w: &World, p: usize, scale: f32) {
    let pl = &w.pilots[p];
    if pl.pvp != cj_sim::perm::Pvp::Enabled || !pl.ship.alive {
        return;
    }
    let b = pl.ship.b;
    let (x, y, r) = (b.x as f32, b.y as f32, b.r as f32 + 10.0);
    draw_circle_lines(x, y, r, 2.0 / scale, crate::look::rgba(col::DANGER, 0.8));
    crate::foes::tag(look, x, y + r + 14.0, "PVP", col::DANGER, scale);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rect's top-left must land at the window's top-left (GL clip
    /// space y = +1), not the bottom-left, or everything draws upside down.
    #[test]
    fn display_camera_keeps_y_down() {
        let m = display_camera(Rect::new(0.0, 0.0, 800.0, 600.0)).matrix();
        let top_left = m.project_point3(vec3(0.0, 0.0, 0.0));
        let bottom_right = m.project_point3(vec3(800.0, 600.0, 0.0));
        assert!(
            (top_left.x + 1.0).abs() < 1e-5 && (top_left.y - 1.0).abs() < 1e-5,
            "{top_left:?}"
        );
        assert!(
            (bottom_right.x - 1.0).abs() < 1e-5 && (bottom_right.y + 1.0).abs() < 1e-5,
            "{bottom_right:?}"
        );
    }
}
