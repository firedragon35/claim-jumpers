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
use cj_sim::consts::*;
use cj_sim::entities::{Item, Rock, ShapePt, Ship, Wreck};
use cj_sim::items::ItemType;
use cj_sim::rng::Rng;
use cj_sim::World;
use macroquad::prelude::*;
use std::f32::consts::TAU;

/* ------------------------------ Helpers ------------------------------ */

fn v(x: f64, y: f64) -> Vec2 {
    vec2(x as f32, y as f32)
}

/// A rigid transform: local tug/rock coordinates to world coordinates.
#[derive(Clone, Copy)]
struct Xf {
    o: Vec2,
    c: f32,
    s: f32,
}

impl Xf {
    fn new(x: f64, y: f64, angle: f64) -> Xf {
        Xf {
            o: v(x, y),
            c: angle.cos() as f32,
            s: angle.sin() as f32,
        }
    }
    fn p(&self, x: f32, y: f32) -> Vec2 {
        self.o + vec2(x * self.c - y * self.s, x * self.s + y * self.c)
    }
    fn pts(&self, local: &[(f32, f32)]) -> Vec<Vec2> {
        local.iter().map(|&(x, y)| self.p(x, y)).collect()
    }
}

/// A line through `pts`, optionally closed. Opaque lines get round joints
/// so corners don't show notches; translucent ones skip them to avoid
/// doubled alpha where segments meet.
fn polyline(pts: &[Vec2], closed: bool, width: f32, color: Color) {
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
fn glow_stroke(pts: &[Vec2], closed: bool, rgb: u32, width: f32, alpha: f32) {
    polyline(pts, closed, width * 3.5, rgba(rgb, alpha * 0.22));
    polyline(pts, closed, width, rgba(rgb, alpha));
}

/// Fill a star-shaped polygon as a fan around `centre`.
fn fill_fan(centre: Vec2, pts: &[Vec2], color: Color) {
    for i in 0..pts.len() {
        draw_triangle(centre, pts[i], pts[(i + 1) % pts.len()], color);
    }
}

/// Fill any simple polygon by ear clipping (the carrier hull is concave).
fn fill_poly(pts: &[Vec2], color: Color) {
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
fn dashed(pts: &[Vec2], closed: bool, dash: f32, gap: f32, offset: f32, width: f32, color: Color) {
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

fn circle_pts(c: Vec2, r: f32, n: usize) -> Vec<Vec2> {
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

    /// Draw in screen space (logical pixels).
    fn draw(&self, look: &Look, cam: Vec2, t: f32, w: f32, h: f32) {
        let span = w.max(h);
        look.additive(|| {
            for &(nx, ny, nr, rgb) in &NEBULAE {
                let x = (nx * w * 1.6 - cam.x * 0.02).rem_euclid(w * 1.6) - w * 0.3;
                let y = (ny * h * 1.6 - cam.y * 0.02).rem_euclid(h * 1.6) - h * 0.3;
                look.glow(x, y, nr * span, rgb, false, 0.22);
            }
        });
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

fn draw_boundary(t: f32) {
    let (w, h) = (WORLD_W as f32, WORLD_H as f32);
    let pts = [vec2(0.0, 0.0), vec2(w, 0.0), vec2(w, h), vec2(0.0, h)];
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
    fill_poly(&hull, Color::from_hex(0x121722));
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
                Color::from_hex(col::DANGER)
            } else {
                Color::from_hex(0x3a1410)
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
        Color::from_hex(if flash { 0x1c3337 } else { col::WRECK_FILL }),
    );
    polyline(
        &pts,
        true,
        2.5,
        Color::from_hex(if flash { col::TEXT } else { col::WRECK }),
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
        draw_circle(p.x, p.y, 4.0, Color::from_hex(col::DANGER));
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
        Color::from_hex(if flash { 0x2b3448 } else { col::ROCK_FILL }),
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
        Color::from_hex(if flash { col::TEXT } else { col::ROCK }),
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

fn draw_item(it: &Item) {
    let xf = Xf::new(it.b.x, it.b.y, it.angle);
    let r = it.b.r as f32;
    match it.kind {
        ItemType::Scrap => {
            let pts = shape_pts(&xf, &it.shape, it.b.r);
            fill_fan(xf.o, &pts, Color::from_hex(0x1a2130));
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
        // Placeholder: wreck salvage (plates, engines, reactors, pods and
        // the rest) can't appear until wrecks open in milestone 2, which
        // brings their artwork. Until then it is a plain outlined disc.
        _ => {
            let c = it.tint.unwrap_or(col::STEEL);
            draw_circle(xf.o.x, xf.o.y, r, Color::from_hex(0x18202e));
            draw_circle_lines(xf.o.x, xf.o.y, r, 2.0, Color::from_hex(c));
        }
    }
}

/// The tow line. A taut link is straight and brightens with tension; a
/// slack link sags into a curve so the rope reads as rope.
fn draw_cable(w: &World) {
    let s = &w.ship;
    if !s.alive || w.chain.is_empty() {
        return;
    }
    let mut a = {
        let (x, y) = s.stern();
        v(x, y)
    };
    for (i, id) in w.chain.iter().enumerate() {
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

/// The stock Mk I tug (drawTug with the cannon mounted and no other parts).
fn draw_ship(look: &Look, s: &Ship, t: f32, rng: &mut Rng) {
    if !s.alive || (s.invuln > 0.0 && ((t * 12.0).floor() as i64) % 2 == 1) {
        return; // respawn flicker
    }
    let xf = Xf::new(s.b.x, s.b.y, s.angle);
    // Exhaust flame, choked while sputtering.
    if s.thrust_level > 0.0 && !s.sputtering {
        let len = rng.range(14.0, 26.0) as f32;
        let g = xf.p(-24.0, 0.0);
        look.additive(|| look.glow(g.x, g.y, 16.0 + len * 0.4, col::HAZARD, true, 1.0));
        draw_triangle(
            xf.p(-20.0, -6.0),
            xf.p(-20.0 - len, 0.0),
            xf.p(-20.0, 6.0),
            rgba(col::HAZARD, 0.85),
        );
        draw_triangle(
            xf.p(-20.0, -3.0),
            xf.p(-20.0 - len * 0.55, 0.0),
            xf.p(-20.0, 3.0),
            Color::from_hex(0xfff3d0),
        );
    }
    // The cannon's barrel.
    let barrel = xf.pts(&[(20.0, -1.5), (32.0, -1.5), (32.0, 1.5), (20.0, 1.5)]);
    fill_fan(
        (barrel[0] + barrel[2]) / 2.0,
        &barrel,
        Color::from_hex(col::TEXT),
    );
    // Hull.
    let hull = xf.pts(&[
        (22.0, -6.0),
        (22.0, 6.0),
        (14.0, 13.0),
        (-14.0, 14.0),
        (-20.0, 8.0),
        (-20.0, -8.0),
        (-14.0, -14.0),
        (14.0, -13.0),
    ]);
    fill_fan(xf.o, &hull, Color::from_hex(0x2a1d06));
    glow_stroke(
        &hull,
        true,
        if s.flash > 0.0 {
            col::DANGER
        } else {
            col::HAZARD
        },
        2.0,
        1.0,
    );
    let cab = xf.pts(&[(6.0, -5.0), (15.0, -5.0), (15.0, 5.0), (6.0, 5.0)]);
    fill_fan((cab[0] + cab[2]) / 2.0, &cab, Color::from_hex(col::CABLE));
    let (a, b) = (xf.p(-6.0, -14.0), xf.p(-6.0, 14.0));
    draw_line(a.x, a.y, b.x, b.y, 1.5, Color::from_hex(col::HAZARD));
    // Stock winch drum.
    polyline(
        &circle_pts(xf.p(-16.0, 0.0), 5.0, 20),
        true,
        1.5,
        Color::from_hex(col::HAZARD),
    );

    // Shield bubble: faint when full, brighter as it takes hits, gone when
    // depleted; a rippling dashed ring while recharging.
    if s.shield > 0.0 {
        let frac = (s.shield / 40.0) as f32;
        let flash = s.shield_flash as f32;
        draw_circle(
            xf.o.x,
            xf.o.y,
            34.0,
            rgba(col::SHIELD, (0.05 + frac * 0.06 + flash * 1.2).min(1.0)),
        );
        let ring = circle_pts(xf.o, 34.0, 64);
        let c = rgba(col::SHIELD, (0.25 + frac * 0.35 + flash * 2.0).min(1.0));
        if s.shield_delay_t <= 0.0 && frac < 1.0 {
            dashed(&ring, true, 6.0, 6.0, t * 30.0, 1.5, c);
        } else {
            polyline(&ring, true, 1.5, c);
        }
    }
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
fn draw_hook_target(look: &Look, w: &World, t: f32, device: Device, scale: f32) {
    let Some(i) = w.find_hook_target() else {
        return;
    };
    let it = &w.items[i];
    let too_heavy = w.tow_mass() + it.b.mass > w.mods.winch_cap;
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
        Color::from_hex(c),
    );
    let key = if device == Device::Pad { "Y" } else { "E" };
    let line1 = if too_heavy {
        "TOO HEAVY".to_owned()
    } else {
        format!("{key} · {}", it.label.to_uppercase())
    };
    text_centered(
        &line1,
        p.x,
        p.y - r - 34.0,
        13.0,
        &look.fonts.mono,
        Color::from_hex(c),
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
        Color::from_hex(c2),
        scale,
    );
}

fn draw_particles(look: &Look, fx: &Fx, visible: impl Fn(f64, f64, f64) -> bool) {
    // Smoke first, in normal blending, so the glowing fire draws over it.
    for p in &fx.particles {
        if p.kind != Kind::Smoke || !visible(p.x, p.y, 0.0) {
            continue;
        }
        let f = (p.life / p.max) as f32;
        let r = p.size as f32 * (1.0 + (1.0 - f) * p.grow as f32);
        draw_circle(p.x as f32, p.y as f32, r, rgba(p.rgb, 0.5 * f));
    }
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

/// Logical-pixel camera over the whole window, for screen-space drawing.
pub fn screen_camera(view: (f32, f32)) {
    set_camera(&Camera2D::from_display_rect(Rect::new(
        0.0, 0.0, view.0, view.1,
    )));
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
    /// Logical viewport size and the display's pixel ratio.
    pub view: (f32, f32),
    pub dpi: f32,
}

pub fn render(sc: Scene) {
    let w = sc.world;
    let t = w.t as f32;
    let (vw, vh) = sc.view;
    clear_background(Color::from_hex(col::VOID));
    screen_camera(sc.view);
    sc.stars.draw(sc.look, v(w.cam.x, w.cam.y), t, vw, vh);

    // World camera, with screen shake.
    let z = w.cam.zoom as f32;
    let shake = w.shake;
    let (sx, sy) = if shake > 0.0 {
        (
            sc.rng.range(-1.0, 1.0) * shake,
            sc.rng.range(-1.0, 1.0) * shake,
        )
    } else {
        (0.0, 0.0)
    };
    let centre = v(w.cam.x + sx, w.cam.y + sy);
    set_camera(&Camera2D::from_display_rect(Rect::new(
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

    draw_boundary(t);
    draw_carrier(sc.look, t, scale);
    for wr in &w.wrecks {
        if visible(wr.b.x, wr.b.y, wr.b.r) {
            draw_wreck(wr, t);
        }
    }
    for rk in &w.rocks {
        if visible(rk.b.x, rk.b.y, rk.b.r) {
            draw_rock(sc.look, rk);
        }
    }
    draw_cable(w);
    for it in &w.items {
        if visible(it.b.x, it.b.y, it.b.r) {
            draw_item(it);
        }
    }
    draw_ship(sc.look, &w.ship, t, sc.rng);
    draw_particles(sc.look, sc.fx, visible);
    for p in &sc.fx.popups {
        let c = rgba(p.rgb, p.life.min(1.0) as f32);
        text_centered(
            &p.text,
            p.x as f32,
            p.y as f32,
            24.0,
            &sc.look.fonts.display,
            c,
            scale,
        );
    }
    if !w.shift_over {
        draw_hook_target(sc.look, w, t, sc.device, scale);
    }

    // Screen-space overlays.
    screen_camera(sc.view);
    sc.vignette.draw(vw, vh);
    // Big nearby explosions wash the screen with light for a moment.
    if w.screen_flash > 0.0 {
        draw_rectangle(
            0.0,
            0.0,
            vw,
            vh,
            Color::new(1.0, 244.0 / 255.0, 220.0 / 255.0, w.screen_flash as f32),
        );
    }
}
