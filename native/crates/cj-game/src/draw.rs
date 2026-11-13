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
    /// The frame's turn, radians.
    pub(crate) fn angle(&self) -> f32 {
        self.s.atan2(self.c)
    }

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

/// Light falls on the field from the upper left, fixed in world space so
/// the lit side stays put while a body spins (as the rock highlight does).
/// `lit` is +1 on an edge facing the light and -1 on one facing away.
const LIGHT_TOWARD: Vec2 = Vec2::new(-0.6, -0.8);

fn lit(centre: Vec2, p: Vec2) -> f32 {
    (p - centre).normalize_or_zero().dot(LIGHT_TOWARD)
}

/// A colour scaled toward brighter (`k` > 1) or darker (`k` < 1).
fn shade(c: Color, k: f32) -> Color {
    Color::new(
        (c.r * k).min(1.0),
        (c.g * k).min(1.0),
        (c.b * k).min(1.0),
        c.a,
    )
}

/// `a` blended toward `b` by `k` (0..1), alpha included.
fn mix_col(a: Color, b: Color, k: f32) -> Color {
    Color::new(
        a.r + (b.r - a.r) * k,
        a.g + (b.g - a.g) * k,
        a.b + (b.b - a.b) * k,
        a.a + (b.a - a.a) * k,
    )
}

/// Fill a star-shaped polygon as a fan whose rim vertices are lit from
/// LIGHT_TOWARD: up to `lift` brighter on the lit side and up to `sink`
/// darker on the far side, blending smoothly across the body (the GPU
/// interpolates vertex colours). One mesh, so no extra draw calls.
fn shaded_fan(centre: Vec2, pts: &[Vec2], base: Color, lift: f32, sink: f32) {
    let n = pts.len();
    if n < 3 || n > u16::MAX as usize - 1 {
        fill_fan(centre, pts, base);
        return;
    }
    let mut vertices = Vec::with_capacity(n + 1);
    vertices.push(Vertex::new(centre.x, centre.y, 0.0, 0.0, 0.0, base));
    for &p in pts {
        let l = lit(centre, p);
        let k = if l > 0.0 {
            1.0 + lift * l
        } else {
            1.0 + sink * l
        };
        vertices.push(Vertex::new(p.x, p.y, 0.0, 0.0, 0.0, shade(base, k)));
    }
    let mut indices = Vec::with_capacity(n * 3);
    for i in 0..n {
        indices.extend_from_slice(&[0, (i + 1) as u16, ((i + 1) % n + 1) as u16]);
    }
    draw_mesh(&Mesh {
        vertices,
        indices,
        texture: None,
    });
}

/// A closed outline lit from LIGHT_TOWARD: edges facing the light are
/// drawn in `rim`, fading through `edge` to `dim` on the far side. Gives
/// solid bodies a lit rim and lets their shadowed outline recede.
fn lit_outline(centre: Vec2, pts: &[Vec2], width: f32, rim: Color, edge: Color, dim: Color) {
    let n = pts.len();
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        let l = lit(centre, (a + b) * 0.5);
        let c = if l > 0.0 {
            mix_col(edge, rim, l)
        } else {
            mix_col(edge, dim, -l)
        };
        draw_line(a.x, a.y, b.x, b.y, width, c);
        // Round joints, so corners don't notch at this width.
        draw_circle(b.x, b.y, width / 2.0, c);
    }
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

/// Four parallax layers of twinkling stars over slow nebula clouds, and
/// a sheet of dust drifting just behind the field.
pub struct Stars {
    layers: Vec<Layer>,
    /// Dust motes; `tw` is each one's drift speed, `ph` its drift heading.
    dust: Vec<Star>,
}

/// Star layers, far to near: (parallax, size px, alpha, count). The far
/// one is many faint pinpricks that barely move, for depth.
const STAR_LAYERS: [(f32, f32, f32, usize); 4] = [
    (0.03, 0.55, 0.22, 170),
    (0.08, 0.7, 0.35, 130),
    (0.2, 1.25, 0.6, 100),
    (0.45, 1.8, 0.85, 70),
];
/// Dust parallax: just behind the field, so it slides past almost with it.
const DUST_PARALLAX: f32 = 0.75;
const DUST_COUNT: usize = 40;

const STAR_TINTS: [u32; 6] = [0xdde5f3, 0xdde5f3, 0xdde5f3, 0xbcd4ff, 0xffe2b8, 0xffd0e4];
const NEBULAE: [(f32, f32, f32, u32); 4] = [
    (0.2, 0.25, 0.75, 0x2a3a7a),
    (0.75, 0.65, 0.9, 0x4a2463),
    (0.45, 0.9, 0.6, 0x14505a),
    (0.95, 0.15, 0.5, 0x5a2a3a),
];

impl Stars {
    pub fn new(rng: &mut Rng) -> Stars {
        let near = STAR_LAYERS.len() - 1;
        let layers = STAR_LAYERS
            .iter()
            .enumerate()
            .map(|(li, &(f, size, alpha, n))| Layer {
                f,
                size,
                alpha,
                stars: (0..n)
                    .map(|_| Star {
                        x: rng.next() as f32,
                        y: rng.next() as f32,
                        rgb: STAR_TINTS[rng.index(STAR_TINTS.len())],
                        tw: rng.range(0.6, 2.2) as f32,
                        ph: rng.range(0.0, std::f64::consts::TAU) as f32,
                        bright: li == near && rng.chance(0.12),
                    })
                    .collect(),
            })
            .collect();
        let dust = (0..DUST_COUNT)
            .map(|_| Star {
                x: rng.next() as f32,
                y: rng.next() as f32,
                rgb: if rng.chance(0.5) { 0x8a8478 } else { 0x6a7488 },
                tw: rng.range(4.0, 12.0) as f32,
                ph: rng.range(0.0, std::f64::consts::TAU) as f32,
                bright: false,
            })
            .collect();
        Stars { layers, dust }
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
        // Dust: soft motes drifting slowly on their own (held still with
        // reduced motion), wrapping a margin beyond the view so they never pop.
        let drift = if crate::look::reduce_motion() { 0.0 } else { t };
        let m = 40.0;
        for d in &self.dust {
            let (dx, dy) = (d.ph.cos() * d.tw * drift, d.ph.sin() * d.tw * drift);
            let x = (d.x * (w + m) * 3.0 - cam.x * DUST_PARALLAX + dx).rem_euclid(w + m) - m / 2.0;
            let y = (d.y * (h + m) * 3.0 - cam.y * DUST_PARALLAX + dy).rem_euclid(h + m) - m / 2.0;
            look.glow(x, y, 3.0 + d.tw * 0.5, d.rgb, false, 0.16);
        }
    }
}

/* ------------------------------ The field ------------------------------ */

/// The field's dashed edge, from `lo` to `hi` on both axes. With effects
/// on, an energy wall glows along it (brighter as `ship` nears it) and the
/// dashes, still marking the exact line, are fainter.
fn draw_boundary(post: &crate::post::Post, view: Rect, ship: Vec2, t: f32, lo: f64, hi: f64) {
    let (lo, hi) = (lo as f32, hi as f32);
    let wall = post.edge_wall(view, lo, hi, ship, t, col::HAZARD);
    let pts = [vec2(lo, lo), vec2(hi, lo), vec2(hi, hi), vec2(lo, hi)];
    dashed(
        &pts,
        true,
        40.0,
        40.0,
        t * 20.0,
        6.0,
        rgba(col::HAZARD, if wall { 0.2 } else { 0.35 }),
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
    if crate::look::sprites() {
        // The hub art over the main hull (its collision circles), with the
        // carrier's name and the way to the bay under it.
        let tex = &look.props.carrier;
        draw_sprite(
            look,
            tex,
            xf.p(15.0, 0.0),
            long_fit(tex, 500.0),
            0.0,
            ship_tint(),
            None,
        );
        let c = xf.p(15.0, 128.0);
        text_centered(
            "HOLLIS-3 · SALVAGE CARRIER · BAY ←",
            c.x,
            c.y,
            14.0,
            &look.fonts.mono,
            rgba(col::HAZARD, 0.8),
            scale,
        );
    } else {
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

fn draw_wreck(look: &Look, wr: &Wreck, t: f32) {
    let xf = Xf::new(wr.b.x, wr.b.y, wr.angle);
    let r = wr.b.r as f32;
    if crate::look::sprites() {
        let art = &look.props;
        // The freighter is a long derelict hull; other wrecks are debris
        // about as big as their hull outline.
        let (tex, size) = if wr.freighter {
            (&art.freighter, long_fit(&art.freighter, r * 2.7))
        } else {
            let tex = &art.wrecks[crate::look::wreck_art(wr.light, art.wrecks.len())];
            (tex, area_fit(tex, r * 0.95))
        };
        draw_sprite(
            look,
            tex,
            xf.o,
            size,
            wr.angle as f32,
            ship_tint(),
            (wr.flash > 0.0).then_some(0.5),
        );
        wreck_damage(&xf, wr, r, t);
        return;
    }
    let pts: Vec<Vec2> = wr
        .pts
        .iter()
        .map(|&(x, y)| xf.p(x as f32, y as f32))
        .collect();
    let flash = wr.flash > 0.0;
    if flash {
        fill_fan(xf.o, &pts, crate::look::hex(0x1c3337));
        polyline(&pts, true, 2.5, crate::look::hex(col::TEXT));
    } else {
        shaded_fan(xf.o, &pts, crate::look::hex(col::WRECK_FILL), 1.1, 0.55);
        // Inner plating: the hull outline inset, so the hull reads as
        // thick armour rather than a thin shell.
        let inner: Vec<Vec2> = pts.iter().map(|&p| xf.o + (p - xf.o) * 0.78).collect();
        polyline(&inner, true, 1.0, rgba(col::WRECK, 0.16));
        let edge = crate::look::hex(col::WRECK);
        lit_outline(
            xf.o,
            &pts,
            2.5,
            mix_col(edge, WHITE, 0.45),
            edge,
            shade(edge, 0.55),
        );
    }
    // Ribs: broken frame lines across the hull.
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
    wreck_damage(&xf, wr, r, t);
}

/// Cracks that deepen with damage, and a dying emergency light.
fn wreck_damage(xf: &Xf, wr: &Wreck, r: f32, t: f32) {
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
fn draw_rock(look: &Look, rk: &Rock, fx: &Fx, belt: cj_sim::field::Belt) {
    let light = fx.light_at(rk.b.x, rk.b.y, rk.b.r);
    if !crate::look::sprites() {
        draw_rock_vector(look, rk);
        if let Some(l) = light {
            blast_lit_rock(look, rk, l);
        }
        draw_cracks(rk);
        return;
    }
    let tex = match crate::look::belt_rock_art(belt, rk.size, rk.veined, rk.id) {
        Some(i) => &look.belt_rocks[i],
        None => &look.rocks[crate::look::rock_art(rk.size, rk.veined, rk.id)],
    };
    let flash = (rk.flash > 0.0).then_some(0.6);
    // The lit shader lights the side facing the blast; a soft glow on that
    // side spills a little light past the rock's edge.
    draw_sprite_lit(
        look,
        tex,
        v(rk.b.x, rk.b.y),
        area_fit(tex, rk.b.r as f32),
        rk.angle as f32,
        rock_tint(rk.veined, 1.0),
        flash,
        light,
    );
    // Bigger ore-bearing rocks show where the ore is: a node set in the
    // rock, mined out once the rock is half broken.
    if rk.veined && rk.size >= 2 {
        let max = cj_sim::consts::rock_size(rk.size).1;
        let node = if rk.hp > max / 2.0 {
            &look.props.ore_node
        } else {
            &look.props.depleted_node
        };
        let a = rk.id as f32 * 2.39;
        let r = rk.b.r as f32;
        let at = Xf::new(rk.b.x, rk.b.y, rk.angle).p(a.cos() * r * 0.3, a.sin() * r * 0.3);
        draw_sprite_lit(
            look,
            node,
            at,
            area_fit(node, r * 0.32),
            rk.angle as f32 + a,
            ship_tint(),
            flash,
            light,
        );
    }
    if let Some((dx, dy, k, rgb)) = light {
        let r = rk.b.r as f32;
        let p = v(rk.b.x + dx * rk.b.r * 0.6, rk.b.y + dy * rk.b.r * 0.6);
        look.additive(|| look.glow(p.x, p.y, r * 0.5, rgb, false, k as f32 * 0.25));
    }
    draw_cracks(rk);
}

/// The bay's tractor beams: a shimmering line from the ring to each piece
/// of cargo it is pulling in, brighter the closer the piece is.
fn draw_tractor(look: &Look, w: &World, t: f32) {
    use cj_sim::consts::{BAY_R, BAY_TRACTOR_REACH, BAY_X, BAY_Y};
    let bay = v(BAY_X, BAY_Y);
    let reach = (BAY_R * BAY_TRACTOR_REACH) as f32;
    for it in &w.items {
        if it.dead || matches!(it.kind, ItemType::Charge | ItemType::Keycard) {
            continue;
        }
        let p = v(it.b.x, it.b.y);
        let d = p.distance(bay);
        if d >= reach {
            continue;
        }
        let k = 1.0 - d / reach;
        let shimmer = 0.75 + 0.25 * (t * 14.0 + it.id as f32).sin();
        let from = bay + (p - bay) * (BAY_R as f32 * 0.85 / d.max(1.0)).min(1.0);
        look.additive(|| {
            draw_line(
                from.x,
                from.y,
                p.x,
                p.y,
                2.0 + 4.0 * k,
                rgba(col::CABLE, 0.18 * k * shimmer),
            );
            draw_line(
                from.x,
                from.y,
                p.x,
                p.y,
                1.0,
                rgba(col::TEXT, 0.35 * k * shimmer),
            );
        });
    }
}

/// How many cracks a rock with `hp` of `max` hit points shows: one per
/// quarter of its hull lost, none while whole.
fn cracks(hp: f64, max: f64) -> usize {
    let lost = (1.0 - hp / max.max(1e-9)).clamp(0.0, 1.0);
    (lost * 4.0).ceil() as usize
}

/// Jagged cracks spreading from a damaged rock's middle, the same ones
/// every frame (seeded by its id) and turning with it.
fn draw_cracks(rk: &Rock) {
    let n = cracks(rk.hp, cj_sim::consts::rock_size(rk.size).1);
    if n == 0 {
        return;
    }
    let xf = Xf::new(rk.b.x, rk.b.y, rk.angle);
    let r = rk.b.r as f32;
    let seed = rk.id as f32 * 0.7548;
    for i in 0..n {
        let a = seed * TAU + i as f32 * 2.39;
        let len = r * (0.45 + 0.3 * (seed * 7.0 + i as f32).sin().abs());
        let pts: Vec<Vec2> = (0..4)
            .map(|k| {
                let u = k as f32 / 3.0;
                let jag = if k % 2 == 1 { 0.25 } else { -0.15 };
                let b = a + jag;
                xf.p(b.cos() * len * u, b.sin() * len * u)
            })
            .collect();
        polyline(&pts, false, 1.6, rgba(0x0a0a10, 0.75));
        polyline(&pts, false, 0.7, rgba(col::HAZARD, 0.35));
    }
}

/// An explosion's flash on a vector rock: the 1.6.0 lighting turned to
/// face the blast, a bright rim on the edges toward it and a glow on that
/// side, in the blast's colour.
fn blast_lit_rock(look: &Look, rk: &Rock, (dx, dy, k, rgb): (f64, f64, f64, u32)) {
    let xf = Xf::new(rk.b.x, rk.b.y, rk.angle);
    let r = rk.b.r as f32;
    let toward = vec2(dx as f32, dy as f32);
    let k = k as f32;
    let p = xf.o + toward * r * 0.35;
    look.additive(|| look.glow(p.x, p.y, r * 0.75, rgb, false, k * 0.45));
    let pts = shape_pts(&xf, &rk.shape, rk.b.r);
    let n = pts.len();
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        let face = ((a + b) * 0.5 - xf.o).normalize_or_zero().dot(toward);
        if face > 0.0 {
            draw_line(a.x, a.y, b.x, b.y, 2.5, rgba(rgb, k * face));
        }
    }
}

/// The original vector rock (Art style: vector).
fn draw_rock_vector(look: &Look, rk: &Rock) {
    let xf = Xf::new(rk.b.x, rk.b.y, rk.angle);
    let r = rk.b.r as f32;
    let pts = shape_pts(&xf, &rk.shape, rk.b.r);
    let flash = rk.flash > 0.0;
    if flash {
        fill_fan(xf.o, &pts, crate::look::hex(0x2b3448));
    } else {
        shaded_fan(xf.o, &pts, crate::look::hex(col::ROCK_FILL), 1.3, 0.6);
    }
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
    if flash {
        polyline(&pts, true, 2.0, crate::look::hex(col::TEXT));
    } else {
        let edge = crate::look::hex(col::ROCK);
        lit_outline(
            xf.o,
            &pts,
            2.0,
            mix_col(edge, WHITE, 0.55),
            edge,
            shade(edge, 0.5),
        );
    }
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

/// The art's lit faces are bright enough to cross the bloom threshold
/// and glow: darken plain rock into the field's blue-grey, and ore a
/// little less so its seams still shine.
fn rock_tint(ore: bool, alpha: f32) -> Color {
    if ore {
        Color::new(0.8, 0.78, 0.76, alpha)
    } else {
        Color::new(0.5, 0.54, 0.62, alpha)
    }
}

/// Ship art is lit brighter than the field: dim it under the bloom
/// threshold.
pub(crate) fn ship_tint() -> Color {
    Color::new(0.7, 0.72, 0.78, 1.0)
}

/// Hull left (0..1) below which a painted ship shows its damage.
const HURT_AT: f64 = 0.4;

/// How badly a ship with `hull` (0..1) of its hull left shows its damage:
/// 0 at `HURT_AT` and above, rising to 1 as the hull runs out.
pub(crate) fn hurt(hull: f64) -> f32 {
    (1.0 - hull.clamp(0.0, 1.0) / HURT_AT).max(0.0) as f32
}

/// A hurt ship's paint, scorched darker.
fn hurt_tint(c: Color, hurt: f32) -> Color {
    let k = 1.0 - 0.35 * hurt;
    Color::new(c.r * k, c.g * k, c.b * k, c.a)
}

/// Smoke drifting off a hurt ship, and now and then a spark from its hull.
/// `seed` keeps each ship's smoke out of step with the others'.
fn draw_hurt(look: &Look, c: Vec2, len: f32, hurt: f32, t: f32, seed: u32) {
    if hurt <= 0.0 {
        return;
    }
    let s = seed as f32 * 1.618;
    for i in 0..3 {
        let p = (t * 0.7 + i as f32 / 3.0 + s).fract();
        let a = s * 2.4 + i as f32 * 2.1;
        let at = c + vec2(a.cos(), a.sin()) * len * (0.1 + 0.55 * p);
        let r = len * (0.12 + 0.22 * p);
        look.glow(at.x, at.y, r, 0x3a3f46, false, (1.0 - p) * 0.55 * hurt);
    }
    if (t * 6.0 + s).fract() < 0.12 + 0.2 * hurt {
        let a = (t * 13.0 + s).floor() * 2.39;
        let at = c + vec2(a.cos(), a.sin()) * len * 0.22;
        look.additive(|| look.glow(at.x, at.y, len * 0.18, col::HAZARD, true, 0.9));
    }
}

/// A tug sprite's longest side, px (the drawn tug was about 48 long).
const TUG_LEN: f32 = 52.0;

/// Painted rivals, law, collectors and haulers are drawn this much larger
/// than their old outlines, so they hold their own next to the tugs.
pub(crate) const OTHER_SHIP_SCALE: f32 = 1.3;

/// Painted salvage is drawn this much larger than its collision circle,
/// so pieces are easy to spot against the rocks.
pub(crate) const ITEM_SCALE: f32 = 1.15;

/// The player and rival art points up the image; the game's angle 0
/// points along +x.
pub(crate) const NOSE_UP: f32 = std::f32::consts::FRAC_PI_2;

/// A sprite size whose area matches the circle of radius `r` the
/// simulation collides with (rocks).
fn area_fit(tex: &Texture2D, r: f32) -> Vec2 {
    tex.size() * (2.1 * r / (tex.width() * tex.height()).sqrt())
}

/// A sprite size whose longest side is `len` (ships).
pub(crate) fn long_fit(tex: &Texture2D, len: f32) -> Vec2 {
    tex.size() * (len / tex.width().max(tex.height()))
}

/// Draw a sprite of `size` centred on `c`, turned by `angle`, through
/// `tint`. `flash` adds that much white light over it (a hit).
pub(crate) fn draw_sprite(
    look: &Look,
    tex: &Texture2D,
    c: Vec2,
    size: Vec2,
    angle: f32,
    tint: Color,
    flash: Option<f32>,
) {
    draw_sprite_lit(look, tex, c, size, angle, tint, flash, None);
}

/// [`draw_sprite`] with a blast's light on it too (see `Fx::light_at`).
#[allow(clippy::too_many_arguments)]
fn draw_sprite_lit(
    look: &Look,
    tex: &Texture2D,
    c: Vec2,
    size: Vec2,
    angle: f32,
    tint: Color,
    flash: Option<f32>,
    blast: Option<(f64, f64, f64, u32)>,
) {
    let params = DrawTextureParams {
        dest_size: Some(size),
        rotation: angle,
        ..Default::default()
    };
    let (x, y) = (c.x - size.x / 2.0, c.y - size.y / 2.0);
    look.lit(angle, blast, || {
        draw_texture_ex(tex, x, y, tint, params.clone())
    });
    if let Some(a) = flash {
        look.additive(|| draw_texture_ex(tex, x, y, Color::new(1.0, 1.0, 1.0, a), params));
    }
}

/// The strongest shockwaves on screen for the composite: centre in 0..1
/// of the view, ring radius and push in view heights. None with reduced motion.
/// Heat haze behind up to two burning engines, yours first, for the
/// composite (see `post::Warp`). None with reduced motion.
fn haze(w: &World, centre: Vec2, z: f32, (vw, vh): (f32, f32)) -> [[f32; 4]; crate::post::HAZE] {
    let mut out = [[0.0; 4]; crate::post::HAZE];
    if crate::look::reduce_motion() {
        return out;
    }
    let me = w.view_pilot;
    let order = std::iter::once(me).chain((0..w.pilots.len()).filter(|&p| p != me));
    let burning = order.filter_map(|p| {
        let s = &w.pilots[p].ship;
        (s.alive && s.throttle > 0.0 && !s.sputtering).then_some(s)
    });
    for (o, s) in out.iter_mut().zip(burning) {
        let back = v(s.b.x - s.angle.cos() * 34.0, s.b.y - s.angle.sin() * 34.0);
        let p = (back - centre) * z;
        let k = s.throttle.min(1.0) as f32;
        *o = [0.5 + p.x / vw, 0.5 + p.y / vh, 16.0 * z / vh, 0.0022 * k];
    }
    out
}

fn ripples(
    fx: &Fx,
    centre: Vec2,
    z: f32,
    (vw, vh): (f32, f32),
) -> [[f32; 4]; crate::post::RIPPLES] {
    let mut out = [[0.0; 4]; crate::post::RIPPLES];
    if crate::look::reduce_motion() {
        return out;
    }
    let mut waves: Vec<[f32; 4]> = fx
        .ripples
        .iter()
        .map(|&(x, y, reach, life, max)| {
            let (r, k) = crate::fx::ripple(reach, life, max);
            let p = (v(x, y) - centre) * z;
            [
                0.5 + p.x / vw,
                0.5 + p.y / vh,
                r as f32 * z / vh,
                k as f32 * 0.012,
            ]
        })
        .collect();
    waves.sort_by(|a, b| b[3].total_cmp(&a[3]));
    for (o, w) in out.iter_mut().zip(waves) {
        *o = w;
    }
    out
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

/// An item in painted style; false for a type with no art, which then
/// draws as vector. Rare pieces sit on a glow of their tint;
/// the reactor's danger ring and the recorder's beacon still show.
fn draw_item_sprite(look: &Look, it: &Item, t: f32) -> bool {
    use crate::look::grade_art;
    let art = &look.props;
    let tex = match it.kind {
        ItemType::Scrap => &art.scrap[grade_art(it.grade, 3)],
        ItemType::Ore => &art.ore[grade_art(it.grade, 4)],
        ItemType::Plate => &art.plate,
        ItemType::Engine => &art.engine[grade_art(it.grade, 3)],
        ItemType::Reactor => &art.reactor[grade_art(it.grade, 3)],
        ItemType::Ingot => &art.ingot[grade_art(it.grade, 3)],
        ItemType::Core => &art.rogue_core,
        ItemType::Blackbox => &art.blackbox,
        ItemType::Pod => &art.pod,
        ItemType::Charge => {
            let warn = it.fuse > 0.0 || it.hp.is_some_and(|hp| hp < 2.0);
            crate::foes::charge(look, &Xf::new(it.b.x, it.b.y, it.angle), t, warn);
            return true;
        }
        ItemType::Keycard => {
            // Blinking so it can be spotted, under the card.
            let p = v(it.b.x, it.b.y);
            let pulse = 10.0 + 5.0 * (t * 4.0).sin().abs();
            look.additive(|| look.glow(p.x, p.y, pulse, col::CABLE, true, 1.0));
            &art.keycard
        }
    };
    let c = v(it.b.x, it.b.y);
    let r = it.b.r as f32;
    if it.rare {
        let rgb = it.tint.unwrap_or(col::STEEL);
        look.additive(|| look.glow(c.x, c.y, r * 1.6 * ITEM_SCALE, rgb, false, 0.5));
    }
    // Plates have one picture for three grades: wash it in the grade's tint.
    let mut tint = ship_tint();
    if it.kind == ItemType::Plate {
        if let Some(rgb) = it.tint {
            tint = mix_col(tint, crate::look::hex(rgb), 0.3);
        }
    }
    draw_sprite(
        look,
        tex,
        c,
        area_fit(tex, r * ITEM_SCALE),
        it.angle as f32,
        tint,
        None,
    );
    match it.kind {
        ItemType::Reactor if it.fuse > 0.0 || it.hp.is_some_and(|hp| hp < 3.0) => {
            draw_circle_lines(c.x, c.y, r + 6.0, 2.0, crate::look::hex(col::DANGER));
        }
        ItemType::Core if (t * 19.0 + it.b.x as f32).sin() > -0.5 => {
            look.additive(|| {
                look.glow(
                    c.x,
                    c.y,
                    9.0 + (t * 7.0).sin() * 3.0,
                    col::DANGER,
                    true,
                    0.7,
                )
            });
        }
        ItemType::Blackbox => {
            let p = Xf::new(it.b.x, it.b.y, it.angle).p(r * 0.8, -r * 0.7);
            let lit = (t * 7.0).sin() > 0.3;
            draw_circle(
                p.x,
                p.y,
                2.5,
                crate::look::hex(if lit { col::DANGER } else { 0x401510 }),
            );
        }
        _ => {}
    }
    true
}

/// The tag over a rival with a temper worth knowing about.
fn temper_tag(t: cj_sim::consts::Temper) -> Option<&'static str> {
    use cj_sim::consts::Temper;
    match t {
        Temper::Steady => None,
        Temper::Greedy => Some("GREEDY"),
        Temper::Skittish => Some("SKITTISH"),
    }
}

fn draw_item(look: &Look, it: &Item, t: f32, scale: f32) {
    if crate::look::sprites() && draw_item_sprite(look, it, t) {
        return;
    }
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
            crate::foes::charge(look, &xf, t, warn);
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
    if crate::look::sprites() {
        if j.hunter {
            // Hunters run hot: they are here for you. Under the hull, so it
            // reads as engine heat.
            let p = xf.p(-8.0, 0.0);
            look.additive(|| look.glow(p.x, p.y, 14.0, col::DANGER, true, 1.0));
        }
        let art = &look.ships;
        let tex = match j.variant {
            Variant::Skiff => &art.skiff,
            Variant::Cutter => &art.cutter,
            Variant::Gunship => &art.gunship,
            Variant::Brute => &art.brute,
        };
        let len = def.r as f32 * 2.4 * k * OTHER_SHIP_SCALE;
        // shortcut: a boss's extra hull isn't known here, so it smokes only
        // below 40% of its variant's base hull; pass the real max if bosses get one.
        let hurt = hurt(j.hp / def.hp);
        draw_sprite(
            look,
            tex,
            v(j.b.x, j.b.y),
            long_fit(tex, len),
            j.angle as f32 + NOSE_UP,
            hurt_tint(ship_tint(), hurt),
            (j.flash > 0.0).then_some(0.6),
        );
        draw_hurt(look, v(j.b.x, j.b.y), len, hurt, w.t as f32, j.id);
    } else {
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
fn draw_cable(look: &Look, w: &World, p: usize) {
    let s = &w.pilots[p].ship;
    // A line anchored to a rock runs from the stern to the rock's near face.
    let rock = s
        .anchor
        .and_then(|(id, _)| w.rocks.iter().find(|r| r.id == id && !r.dead));
    if let (true, Some(r)) = (s.alive, rock) {
        let (x, y) = s.stern();
        let (a, c) = (v(x, y), v(r.b.x, r.b.y));
        let face = c + (a - c).normalize_or_zero() * r.b.r as f32 * 0.8;
        glow_stroke(&[a, face], false, col::CABLE, 2.2, 1.0);
    }
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
        let width = 1.6 + tns * 1.4;
        // (Every pilot's chain; rivals draw their line in draw_jumper.)
        glow_stroke(&pts, false, col::CABLE, width, 1.0);
        // As it tightens the line whitens smoothly (0.3 to 0.9 tension)...
        let hot = ((tns - 0.3) / 0.6).clamp(0.0, 1.0);
        if hot > 0.0 {
            polyline(&pts, false, width, rgba(col::TEXT, hot));
        }
        // A hard yank turns it red and makes it flicker: the cargo is
        // rattling and swinging wide.
        let strain = strain(tns);
        if strain > 0.0 && ((w.t * 18.0).sin() > -0.3 || crate::look::reduce_motion()) {
            polyline(&pts, false, width + 0.6, rgba(col::DANGER, strain));
        }
        // ...and a halo of light builds around it.
        if tns > 0.2 {
            look.additive(|| {
                polyline(
                    &pts,
                    false,
                    6.0 + tns * 10.0,
                    rgba(col::CABLE, 0.14 * tns * tns),
                );
            });
        }
        a = b;
    }
}

/// How strained a cable link at `tension` (0..1) looks: none up to
/// three-quarters, then rising to 1 at full tension.
fn strain(tension: f32) -> f32 {
    ((tension - 0.75) / 0.25).clamp(0.0, 1.0)
}

/// The shield's colour. Painted ships keep their own colours, so the
/// pilot's paint job (or online slot colour) goes on the shield instead;
/// vector tugs wear their paint and keep the shield blue.
fn shield_colour(paint: Option<(u32, u32)>, painted: bool) -> u32 {
    match paint {
        Some((_, trim)) if painted => trim,
        _ => col::SHIELD,
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
    let shield_rgb = shield_colour(paint, crate::look::sprites());
    if crate::look::sprites() {
        // Each pilot slot flies its own model; bought parts show in the shop.
        let art = &look.ships.players[crate::look::player_model(p)];
        tug::draw_flame(look, &f, &st, rng);
        let hurt = hurt(s.hp / w.pilots[p].mods.max_hp);
        draw_sprite(
            look,
            art,
            v(s.b.x, s.b.y),
            long_fit(art, TUG_LEN),
            s.angle as f32 + NOSE_UP,
            hurt_tint(ship_tint(), hurt),
            st.flash.then_some(0.6),
        );
        tug::nozzle_glow(look, &f, &st);
        draw_hurt(look, v(s.b.x, s.b.y), TUG_LEN, hurt, t, p as u32);
    } else {
        tug::draw_tug(look, &f, &w.pilots[p].mods, &st, rng);
    }
    tug::draw_shield(
        s.b.x as f32,
        s.b.y as f32,
        s.shield,
        w.pilots[p].mods.shield_max,
        s.shield_flash,
        s.shield_delay_t,
        t,
        shield_rgb,
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
/// A radiation cloud: churning gas with effects on (a flat green wash
/// without), and a marching edge at the exact hazard radius, fading as it
/// ends.
fn draw_radiation(look: &Look, post: &crate::post::Post, cl: &RadCloud, t: f32, scale: f32) {
    const GREEN: u32 = 0x7dff5a;
    let fade = (cl.t / 3.0).min(1.0) as f32;
    let (p, r) = (v(cl.x, cl.y), RAD_CLOUD_RADIUS as f32);
    if !post.radiation(p, r, t, fade) {
        draw_circle(p.x, p.y, r, rgba(GREEN, 0.10 * fade));
    }
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
    // Smoke and rock chunks first, in normal blending, so the glowing fire
    // draws over them.
    for p in &fx.particles {
        if !matches!(p.kind, Kind::Smoke | Kind::Chunk) || !visible(p.x, p.y, p.size) {
            continue;
        }
        let f = (p.life / p.max) as f32;
        if p.kind == Kind::Chunk {
            draw_chunk(look, p, f);
            continue;
        }
        let r = p.size as f32 * (1.0 + (1.0 - f) * p.grow as f32);
        draw_circle(p.x as f32, p.y as f32, r, rgba(p.rgb, 0.5 * f));
    }
    between();
    // Everything that glows adds light, so overlaps brighten toward white.
    look.additive(|| {
        for p in &fx.particles {
            if matches!(p.kind, Kind::Smoke | Kind::Chunk) || !visible(p.x, p.y, p.ring) {
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

/// A broken-off piece of rock: a lumpy pentagon that tumbles as it flies,
/// lit like the rocks, fading out over its last 40% of life (`f` is the
/// life left, 1 to 0).
fn draw_chunk(look: &Look, p: &crate::fx::Particle, f: f32) {
    // Fixed lumps, so each chunk keeps its shape as it turns.
    const LUMPS: [f32; 5] = [1.0, 0.7, 0.95, 0.6, 0.85];
    let age = (p.max - p.life) as f32;
    let angle = p.ring as f32 + p.grow as f32 * age;
    let c = v(p.x, p.y);
    let r = p.size as f32;
    let fade = (f / 0.4).min(1.0);
    if crate::look::sprites() && (p.rgb == col::ROCK || p.rgb == col::ORE) {
        // Rock breaks into small rocks; the start angle picks which.
        let id = (p.ring * 1000.0) as u32;
        let tex = &look.rocks[crate::look::rock_art(1, p.rgb == col::ORE, id)];
        draw_sprite(
            look,
            tex,
            c,
            area_fit(tex, r),
            angle,
            rock_tint(p.rgb == col::ORE, fade),
            None,
        );
        return;
    }
    let pts: Vec<Vec2> = LUMPS
        .iter()
        .enumerate()
        .map(|(i, &m)| {
            let a = angle + i as f32 * TAU / 5.0;
            c + vec2(a.cos(), a.sin()) * r * m
        })
        .collect();
    let mut fill = crate::look::hex(col::ROCK_FILL);
    fill.a = fade;
    shaded_fan(c, &pts, fill, 1.3, 0.6);
    let mut edge = crate::look::hex(p.rgb);
    edge.a = fade;
    polyline(&pts, true, 1.5, edge);
}

/// Each tug's thruster trail: a ribbon from the stern that thins and fades
/// with age, drawn as light. `t` is the world time the trail points carry.
fn draw_engine_trails(look: &Look, fx: &Fx, t: f64) {
    look.additive(|| {
        for tr in &fx.engine {
            for i in 1..tr.len() {
                let (a, b) = (tr[i - 1], tr[i]);
                // Long jumps (a respawn, a snapshot correction) aren't trail.
                if (b.0 - a.0).hypot(b.1 - a.1) > 80.0 {
                    continue;
                }
                let k = (1.0 - (t - b.2) / crate::fx::ENGINE_TRAIL_TIME).clamp(0.0, 1.0) as f32;
                let (a, b) = (v(a.0, a.1), v(b.0, b.1));
                draw_line(
                    a.x,
                    a.y,
                    b.x,
                    b.y,
                    2.0 + 8.0 * k,
                    rgba(col::HAZARD, 0.22 * k * k),
                );
                draw_line(
                    a.x,
                    a.y,
                    b.x,
                    b.y,
                    1.0 + 2.0 * k,
                    rgba(0xfff3b0, 0.5 * k * k),
                );
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

    let view_rect = Rect::new(
        centre.x - vw / 2.0 / z,
        centre.y - vh / 2.0 / z,
        vw / z,
        vh / z,
    );
    let ship = if w.me().ship.alive {
        v(w.me().ship.b.x, w.me().ship.b.y)
    } else {
        v(w.cam.x, w.cam.y)
    };
    draw_boundary(sc.post, view_rect, ship, t, w.edge_lo, w.edge_hi);
    draw_carrier(sc.look, t, scale);
    // The refinery's pull field can be larger than the screen.
    if let Some(r) = &w.refinery {
        crate::foes::refinery(sc.look, r, t, scale);
    }
    for wr in &w.wrecks {
        if visible(wr.b.x, wr.b.y, wr.b.r) {
            draw_wreck(sc.look, wr, t);
            let (x, y) = (wr.b.x as f32, (wr.b.y - wr.b.r * 0.8 - 14.0) as f32);
            if wr.freighter {
                crate::foes::tag(sc.look, x, y, "DERELICT FREIGHTER", col::ORE, scale);
            } else if wr.locked {
                let s = "SEALED HOLD · NEEDS KEYCARD";
                crate::foes::tag(sc.look, x, y, s, col::CABLE, scale);
            } else if let Some(p) = wr.claim {
                let (s, tint) = if p == w.view_pilot {
                    ("YOUR CLAIM", col::CABLE)
                } else {
                    ("CLAIMED", col::MUTED)
                };
                crate::foes::tag(sc.look, x, y, s, tint, scale);
            }
        }
    }
    for rk in &w.rocks {
        if visible(rk.b.x, rk.b.y, rk.b.r) {
            draw_rock(sc.look, rk, sc.fx, w.belt);
        }
    }
    if sc.show_tug {
        for p in 0..w.pilots.len() {
            draw_cable(sc.look, w, p);
        }
    }
    draw_tractor(sc.look, w, t);
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
            if it.contraband() {
                let (x, y) = (it.b.x as f32, it.b.y as f32 - 22.0);
                crate::foes::tag(sc.look, x, y, "CONTRABAND", col::DANGER, scale);
            }
            if it.kind == ItemType::Keycard && !it.towed {
                let (x, y) = (it.b.x as f32, it.b.y as f32 - 22.0);
                crate::foes::tag(sc.look, x, y, "KEYCARD", col::CABLE, scale);
            }
        }
    }
    for cl in &w.clouds {
        if visible(cl.x, cl.y, RAD_CLOUD_RADIUS) {
            draw_radiation(sc.look, sc.post, cl, t, scale);
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
            let (x, y) = (j.b.x as f32, j.b.y as f32 - 38.0);
            if let Some(b) = &j.boss {
                let name = b.name.to_uppercase();
                crate::foes::tag(sc.look, x, y, &name, col::HAZARD, scale);
            } else if let Some(t) = temper_tag(j.temper()) {
                crate::foes::tag(sc.look, x, y, t, col::MUTED, scale);
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
        draw_engine_trails(sc.look, sc.fx, w.t);
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
    sc.post.finish(
        sc.view,
        seed,
        crate::post::Warp {
            ripples: ripples(sc.fx, centre, z, sc.view),
            haze: haze(w, centre, z, sc.view),
            t,
        },
    );

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
    let f = tug::Frame::new(p.b.x as f32, p.b.y as f32, p.angle as f32, 0.8);
    let st = tug::TugState {
        t,
        thrust: p.thrust as f32,
        ..tug::TugState::default()
    };
    if crate::look::sprites() {
        let art = &look.ships.players[crate::look::player_model(1)];
        tug::draw_flame(look, &f, &st, rng);
        draw_sprite(
            look,
            art,
            v(p.b.x, p.b.y),
            long_fit(art, TUG_LEN * 0.8),
            p.angle as f32 + NOSE_UP,
            ship_tint(),
            None,
        );
    } else {
        let mods = cj_sim::mods::compute_mods(
            &[Some(cj_sim::parts::Part::Cannon), None, None, None, None],
            &cj_sim::parts::Levels::default(),
            1,
        );
        tug::draw_tug(look, &f, &mods, &st, rng);
    }
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

    /// In painted style the chosen paint's trim colours the shield; with no
    /// paint, or in vector style, the shield stays the shield colour.
    #[test]
    fn paint_colours_the_shield_in_painted_style() {
        let paint = Some((0x2a2206, 0xffd23d));
        assert_eq!(shield_colour(paint, true), 0xffd23d);
        assert_eq!(shield_colour(paint, false), col::SHIELD);
        assert_eq!(shield_colour(None, true), col::SHIELD);
    }

    /// The star layers run far to near, each moving more and showing
    /// fewer, bigger stars; the dust moves more than any star but less than
    /// the field.
    #[test]
    fn star_layers_deepen_and_dust_sits_behind_the_field() {
        let stars = Stars::new(&mut Rng::seeded(5));
        for pair in stars.layers.windows(2) {
            assert!(pair[0].f < pair[1].f && pair[0].size < pair[1].size);
            assert!(pair[0].stars.len() > pair[1].stars.len());
        }
        let nearest = stars.layers.last().expect("layers").f;
        assert!(nearest < DUST_PARALLAX && DUST_PARALLAX < 1.0);
        assert_eq!(stars.dust.len(), DUST_COUNT);
    }

    /// A whole rock shows no cracks; each quarter of hull lost adds one.
    #[test]
    fn rocks_crack_as_they_are_mined() {
        assert_eq!(cracks(7.0, 7.0), 0);
        assert_eq!(cracks(6.0, 7.0), 1);
        assert_eq!(cracks(3.5, 7.0), 2);
        assert_eq!(cracks(0.1, 7.0), 4);
        assert_eq!(cracks(-2.0, 7.0), 4);
    }

    /// The cable only shows strain near full tension.
    #[test]
    fn the_cable_shows_strain_only_when_yanked_hard() {
        assert_eq!(strain(0.5), 0.0);
        assert_eq!(strain(0.75), 0.0);
        assert!(strain(0.9) > 0.0 && strain(0.9) < 1.0);
        assert_eq!(strain(1.0), 1.0);
    }

    /// Ships show no damage down to 40% hull, then more and more of it.
    #[test]
    fn ships_show_damage_below_forty_percent_hull() {
        assert_eq!(hurt(1.0), 0.0);
        assert_eq!(hurt(HURT_AT), 0.0);
        assert!(hurt(0.3) > 0.0 && hurt(0.3) < hurt(0.1));
        assert_eq!(hurt(0.0), 1.0);
        assert_eq!(hurt(-5.0), 1.0);
        assert!(hurt_tint(WHITE, 1.0).r < hurt_tint(WHITE, 0.0).r);
    }

    /// Quiet Drift and the Frontier draw some plain rocks with their own
    /// art, always the same art for a rock, every size; ore-bearing rocks
    /// and the other belts keep the usual art.
    #[test]
    fn some_belts_have_their_own_rocks() {
        use crate::look::belt_rock_art;
        use cj_sim::field::Belt;
        for belt in [Belt::Quiet, Belt::Frontier] {
            for size in 1..=3 {
                let own = (0..40)
                    .filter_map(|id| belt_rock_art(belt, size, false, id))
                    .count();
                assert_eq!(own, 20, "{belt:?} {size}");
                assert_eq!(
                    belt_rock_art(belt, size, false, 6),
                    belt_rock_art(belt, size, false, 6)
                );
                assert!((0..40).all(|id| belt_rock_art(belt, size, true, id).is_none()));
            }
        }
        for belt in [Belt::Hollis, Belt::Kessler, Belt::Cinder] {
            assert!((0..40).all(|id| belt_rock_art(belt, 2, false, id).is_none()));
        }
    }

    /// The painted keycard is a card: clear in its rounded corners, solid
    /// in its middle, with a gold chip.
    #[test]
    fn the_keycard_is_painted_with_clear_corners() {
        let img = crate::look::keycard_image();
        assert_eq!((img.width, img.height), (128, 84));
        assert_eq!(img.get_pixel(0, 0).a, 0.0);
        assert_eq!(img.get_pixel(127, 83).a, 0.0);
        assert_eq!(img.get_pixel(64, 50).a, 1.0);
        let chip = img.get_pixel(25, 52);
        assert!(chip.r > chip.b, "{chip:?}");
    }

    /// Grades map onto their art in order, and a grade beyond the art
    /// falls back to the last picture instead of panicking.
    #[test]
    fn grades_and_wrecks_pick_art_safely() {
        use crate::look::{grade_art, wreck_art};
        assert_eq!(
            (0..4).map(|g| grade_art(g, 4)).collect::<Vec<_>>(),
            [0, 1, 2, 3]
        );
        assert_eq!(grade_art(7, 3), 2);
        assert_eq!(grade_art(0, 0), 0);
        // A wreck keeps one piece; different wrecks spread over the set.
        assert_eq!(wreck_art(1.234, 5), wreck_art(1.234, 5));
        let seen: std::collections::HashSet<usize> =
            (0..50).map(|i| wreck_art(i as f64 * 0.137, 5)).collect();
        assert_eq!(seen.len(), 5);
        assert!(wreck_art(-3.9, 5) < 5);
        assert_eq!(wreck_art(2.0, 0), 0);
    }

    /// The first five pilot slots all fly different models, and later
    /// slots take turns with them.
    #[test]
    fn pilot_slots_take_turns_with_the_player_models() {
        use crate::look::player_model;
        let first: std::collections::HashSet<usize> = (0..5).map(player_model).collect();
        assert_eq!(first.len(), 5);
        for slot in 0..40 {
            assert!(player_model(slot) < 5);
            assert_eq!(player_model(slot + 5), player_model(slot));
        }
    }

    /// Every rock gets art of its own size and kind, keeps the same art
    /// for the same id, and every variant gets used.
    #[test]
    fn rocks_pick_art_by_size_kind_and_id() {
        use crate::look::rock_art;
        for size in 1..=3u8 {
            for ore in [false, true] {
                let picks: std::collections::HashSet<usize> =
                    (0..50).map(|id| rock_art(size, ore, id)).collect();
                assert!(!picks.is_empty());
                for id in 0..50 {
                    assert_eq!(rock_art(size, ore, id), rock_art(size, ore, id));
                }
                // An ore rock never borrows plain art, or the reverse.
                for other in (0..50).map(|id| rock_art(size, !ore, id)) {
                    assert!(!picks.contains(&other), "size {size}");
                }
            }
        }
        assert_eq!(
            (0..20)
                .map(|id| rock_art(1, false, id))
                .collect::<std::collections::HashSet<_>>()
                .len(),
            10
        );
        // Out-of-range sizes fall back to the nearest real one.
        assert_eq!(rock_art(0, true, 7), rock_art(1, true, 7));
        assert_eq!(rock_art(9, false, 7), rock_art(3, false, 7));
    }

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

    /// The light sits up and to the left: edges facing it are lit, the
    /// far side is shadowed, and shading never pushes a channel past 1.
    #[test]
    fn bodies_are_lit_from_the_upper_left() {
        let c = vec2(0.0, 0.0);
        assert!(lit(c, vec2(-10.0, -10.0)) > 0.9);
        assert!(lit(c, vec2(10.0, 10.0)) < -0.9);
        assert!(lit(c, vec2(10.0, -10.0)).abs() < 0.2);
        // A point at the centre has no direction: unlit, not NaN.
        assert_eq!(lit(c, c), 0.0);
        let s = shade(Color::new(0.8, 0.5, 0.2, 0.7), 2.0);
        assert_eq!((s.r, s.g, s.b, s.a), (1.0, 1.0, 0.4, 0.7));
        let m = mix_col(BLACK, WHITE, 0.25);
        assert!((m.r - 0.25).abs() < 1e-6 && m.a == 1.0);
    }
}
