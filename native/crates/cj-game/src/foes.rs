//! The opposition drawn as the browser draws it (src/js/17-rendering.js):
//! the Kessler refinery, security drones, debt collectors and their nets,
//! the law, mining drones, missiles and sentry drones, plus the net and
//! cuff meter on the tug.

use crate::controls::Device;
use crate::draw::{
    arc_pts, circle_pts, dashed, fill_convex, fill_fan, fill_poly, glow_stroke, polyline, rect_pts,
    v, Xf,
};
use crate::look::{col, rgba, text_centered, Look};
use cj_sim::opposition::*;
use cj_sim::rng::Rng;
use cj_sim::World;
use macroquad::prelude::*;
use std::f32::consts::{PI, TAU};

fn c(rgb: u32) -> Color {
    crate::look::hex(rgb)
}

/// drawTag: a small label on a dark plate, in world space.
pub fn tag(look: &Look, x: f32, y: f32, s: &str, rgb: u32, scale: f32) {
    let w = crate::ui::text_w(look, s, 11.0, crate::ui::Which::Mono) + 10.0;
    draw_rectangle(
        x - w / 2.0,
        y - 8.0,
        w,
        16.0,
        Color::new(7.0 / 255.0, 10.0 / 255.0, 18.0 / 255.0, 0.7),
    );
    text_centered(s, x, y, 11.0, &look.fonts.mono, c(rgb), scale);
}

/// A mining charge canister in local coordinates, with a blinking light.
pub fn charge(xf: &Xf, t: f32, warn: bool) {
    let body = rect_pts(xf, -13.0, -8.0, 26.0, 16.0);
    fill_convex(&body, c(0x3a0f0a));
    polyline(&body, true, 2.0, c(col::DANGER));
    for x in [-9.0, -1.0, 7.0] {
        fill_convex(&rect_pts(xf, x, -8.0, 3.0, 16.0), c(col::HAZARD));
    }
    let lit = (t * if warn { 20.0 } else { 6.0 }).sin() > 0.0;
    let p = xf.p(0.0, 0.0);
    draw_circle(p.x, p.y, 3.5, c(if lit { col::DANGER } else { 0x5a1a10 }));
}

/// The Kessler ore works: hub, vault, sockets, and the jaws on its -x side.
pub fn refinery(look: &Look, r: &Refinery, t: f32, scale: f32) {
    let hungry = r.state == RefineryState::Hungry;
    let (mx, my) = r.maw();
    let m = v(mx, my);
    if r.state == RefineryState::Open || hungry {
        let pct = js_round((r.jaw_hp / JAW_HP * 100.0).max(0.0));
        tag(
            look,
            m.x,
            m.y - 120.0,
            &format!("JAWS {pct}%"),
            col::DANGER,
            scale,
        );
    }
    // Pull field: rings contracting toward the maw while feeding.
    if hungry {
        for i in 0..4 {
            let f = 1.0 - ((t * 0.35 + i as f32 / 4.0) % 1.0);
            let a = 0.18 * r.hunger as f32 * (1.0 - f * 0.5);
            let rad = MAW_R as f32 + f * (r.pull_r - MAW_R) as f32;
            polyline(&circle_pts(m, rad, 72), true, 2.0, rgba(col::DANGER, a));
        }
    }
    let xf = Xf::new(r.b.x, r.b.y, r.angle);
    // Jaws: two toothed arms hinged behind the hub; awake they gape and
    // snap shut whenever something is eaten.
    let wrecked = r.state == RefineryState::Wrecked;
    let gape = if wrecked {
        0.55
    } else if hungry {
        if r.chomp > 0.0 {
            0.05
        } else {
            0.12 + 0.38 * (0.5 + 0.5 * (t * 2.4).sin())
        }
    } else if r.state == RefineryState::Open {
        0.18
    } else {
        0.04
    };
    for side in [-1.0f32, 1.0] {
        // translate(-HUB_R + 10, side * 40), rotate(-side * gape).
        let (ca, sa) = ((-side * gape).cos(), (-side * gape).sin());
        let hinge = (-REFINERY_HUB_R as f32 + 10.0, side * 40.0);
        let local = |x: f32, y: f32| xf.p(hinge.0 + x * ca - y * sa, hinge.1 + x * sa + y * ca);
        let mut pts = vec![
            local(0.0, side * 30.0),
            local(-215.0, side * 20.0),
            local(-240.0, -side * 5.0),
        ];
        let mut x = -215.0;
        while x <= -25.0 {
            pts.push(local(x, -side * 15.0));
            pts.push(local(x + 12.0, -side * 34.0));
            x += 24.0;
        }
        pts.push(local(0.0, -side * 15.0));
        fill_poly(&pts, c(if wrecked { 0x0d0b0a } else { 0x17110f }));
        let edge = if r.jaw_flash > 0.0 {
            col::TEXT
        } else if wrecked {
            0x3a2f2a
        } else if hungry {
            col::DANGER
        } else {
            0x5b6578
        };
        glow_stroke(&pts, true, edge, 2.5, 1.0);
    }
    // Maw glow between the jaws.
    let mc = xf.p(-MAW_OFFSET as f32, 0.0);
    let glow = if hungry {
        rgba(col::DANGER, 0.15 + 0.15 * (t * 8.0).sin())
    } else {
        Color::new(91.0 / 255.0, 101.0 / 255.0, 120.0 / 255.0, 0.08)
    };
    draw_circle(mc.x, mc.y, MAW_R as f32 * 0.6, glow);

    // Hub: an armoured octagon with hazard banding.
    let hub_r = REFINERY_HUB_R as f32;
    let hub: Vec<Vec2> = (0..8)
        .map(|i| {
            let a = i as f32 / 8.0 * TAU + PI / 8.0;
            xf.p(a.cos() * hub_r, a.sin() * hub_r)
        })
        .collect();
    fill_fan(xf.o, &hub, c(0x10141d));
    glow_stroke(&hub, true, col::STEEL, 2.5, 1.0);
    let band = arc_pts(&xf, 0.0, 0.0, hub_r - 18.0, 0.0, TAU, 96);
    dashed(&band, false, 14.0, 14.0, 0.0, 5.0, rgba(col::HAZARD, 0.5));
    let label = xf.p(0.0, -100.0);
    text_centered(
        "KESSLER ORE WORKS",
        label.x,
        label.y,
        22.0,
        &look.fonts.display,
        Color::new(159.0 / 255.0, 180.0 / 255.0, 207.0 / 255.0, 0.7),
        scale,
    );
    // Vault: sealed plates, or blown open and glowing.
    let vault = arc_pts(&xf, 0.0, 0.0, 64.0, 0.0, TAU, 48);
    if r.state == RefineryState::Open || hungry {
        fill_fan(
            xf.o,
            &vault,
            Color::new(1.0, 210.0 / 255.0, 74.0 / 255.0, 0.18),
        );
        glow_stroke(&vault, true, col::ORE, 2.0, 1.0);
        polyline(
            &arc_pts(&xf, -30.0, 0.0, 50.0, PI * 0.6, PI * 1.4, 16),
            false,
            6.0,
            c(0x5b6578),
        );
        polyline(
            &arc_pts(&xf, 30.0, 0.0, 50.0, -PI * 0.4, PI * 0.4, 16),
            false,
            6.0,
            c(0x5b6578),
        );
    } else {
        fill_fan(xf.o, &vault, c(0x1d222d));
        glow_stroke(&vault, true, col::ORE, 2.5, 1.0);
        let gold = Color::new(1.0, 210.0 / 255.0, 74.0 / 255.0, 0.5);
        polyline(&xf.pts(&[(-50.0, 0.0), (50.0, 0.0)]), false, 4.0, gold);
        polyline(&xf.pts(&[(0.0, -50.0), (0.0, 50.0)]), false, 4.0, gold);
        for i in 0..8 {
            let a = i as f32 / 8.0 * TAU;
            let p = xf.p(a.cos() * 54.0, a.sin() * 54.0);
            draw_circle(p.x, p.y, 3.0, c(col::ORE));
        }
    }
    // Sockets: empty brackets, seated charges, or scorched after the blast.
    for (k, sk) in r.sockets.iter().enumerate() {
        let (px, py) = r.socket_pos(k);
        let sxf = Xf::new(px, py, r.angle + sk.a + std::f64::consts::FRAC_PI_2);
        if sk.filled {
            charge(&sxf, t, r.state == RefineryState::Armed);
        } else if sk.spent {
            let p = sxf.p(0.0, 0.0);
            draw_circle(p.x, p.y, 20.0, c(0x20160f));
        } else {
            let p = sxf.p(0.0, 0.0);
            let ring = circle_pts(p, 24.0, 40);
            dashed(&ring, true, 6.0, 5.0, -t * 20.0, 2.0, c(col::HAZARD));
            polyline(
                &sxf.pts(&[(-8.0, 0.0), (8.0, 0.0)]),
                false,
                2.0,
                c(col::HAZARD),
            );
            polyline(
                &sxf.pts(&[(0.0, -8.0), (0.0, 8.0)]),
                false,
                2.0,
                c(col::HAZARD),
            );
        }
    }
    // Countdown, kept upright in world space.
    if r.state == RefineryState::Armed {
        let s = format!("{:.1}", r.arm_t.max(0.0));
        let y = (r.b.y - REFINERY_HUB_R - 70.0) as f32;
        text_centered(
            &s,
            r.b.x as f32,
            y,
            64.0,
            &look.fonts.display,
            c(col::DANGER),
            scale,
        );
    }
}

/// The browser's Math.round for a percentage label.
fn js_round(x: f64) -> i64 {
    cj_sim::js::round(x) as i64
}

/// Draws a refinery security drone: a dark diamond with a pulsing red eye. `t` is the animation clock in seconds; the outline turns white while the drone flashes from a hit.
pub fn drone(look: &Look, d: &Drone, t: f32) {
    let xf = Xf::new(d.b.x, d.b.y, d.angle);
    let pts = xf.pts(&[(15.0, 0.0), (0.0, -11.0), (-11.0, 0.0), (0.0, 11.0)]);
    fill_convex(&pts, c(0x1b1f28));
    glow_stroke(
        &pts,
        true,
        if d.flash > 0.0 { col::TEXT } else { col::STEEL },
        1.8,
        1.0,
    );
    let p = xf.p(3.0, 0.0);
    let a = 0.6 + 0.4 * (t * 6.0 + d.orbit as f32).sin();
    draw_circle(p.x, p.y, 3.5, rgba(col::DANGER, a));
    let _ = look;
}

/// Draws a debt collector, plus the tether to the tug while the collector holds it in a net. `t` is the animation clock in seconds.
pub fn collector(look: &Look, w: &World, k: &Collector, t: f32, rng: &mut Rng, scale: f32) {
    let s = &w.ship;
    // The tether to a netted tug.
    if k.state == CollectorState::Tow && s.alive && s.net.is_some_and(|n| n.by == k.id) {
        let pts = [v(k.b.x, k.b.y), v(s.b.x, s.b.y)];
        dashed(
            &pts,
            false,
            10.0,
            6.0,
            -t * 40.0,
            2.0 * 3.5,
            rgba(col::COLLECTOR, 0.22),
        );
        dashed(&pts, false, 10.0, 6.0, -t * 40.0, 2.0, c(col::COLLECTOR));
    }
    let xf = Xf::new(k.b.x, k.b.y, k.angle);
    let hull = xf.pts(&[
        (24.0, -8.0),
        (24.0, 8.0),
        (12.0, 18.0),
        (-18.0, 18.0),
        (-26.0, 9.0),
        (-26.0, -9.0),
        (-18.0, -18.0),
        (12.0, -18.0),
    ]);
    fill_fan(xf.o, &hull, c(0x1a1530));
    glow_stroke(
        &hull,
        true,
        if k.flash > 0.0 {
            col::TEXT
        } else {
            col::COLLECTOR
        },
        2.2,
        1.0,
    );
    // Net launcher: a flared mouth on the bow.
    polyline(
        &xf.pts(&[(24.0, -6.0), (34.0, -11.0), (34.0, 11.0), (24.0, 6.0)]),
        false,
        2.0,
        c(col::COLLECTOR),
    );
    // Armour seams and a stencilled lien mark.
    let seam = rgba(col::COLLECTOR, 0.4);
    polyline(&xf.pts(&[(-8.0, -18.0), (-8.0, 18.0)]), false, 1.0, seam);
    polyline(&xf.pts(&[(6.0, -18.0), (6.0, 18.0)]), false, 1.0, seam);
    let lien = xf.p(-1.0, 0.0);
    text_centered(
        "LIEN",
        lien.x,
        lien.y,
        9.0,
        &look.fonts.mono,
        c(col::COLLECTOR),
        scale,
    );
    // Engines glow hard while hauling.
    if k.state == CollectorState::Tow {
        let back = rng.range(0.0, 8.0) as f32;
        fill_convex(
            &rect_pts(&xf, -34.0 - back, -7.0, 8.0, 14.0),
            rgba(col::COLLECTOR, 0.8),
        );
    }
}

/// A spinning hexagonal net, in flight or wrapped on the tug.
pub fn net_mesh(x: f32, y: f32, r: f32, spin: f32, alpha: f32) {
    let xf = Xf::new(f64::from(x), f64::from(y), f64::from(spin));
    let col_ = rgba(col::COLLECTOR, alpha);
    for i in 0..6 {
        let a = i as f32 / 6.0 * TAU;
        polyline(
            &[xf.p(0.0, 0.0), xf.p(a.cos() * r, a.sin() * r)],
            false,
            1.5,
            col_,
        );
    }
    for f in [0.5, 1.0] {
        let ring: Vec<Vec2> = (0..=6)
            .map(|i| {
                let a = i as f32 / 6.0 * TAU;
                xf.p(a.cos() * r * f, a.sin() * r * f)
            })
            .collect();
        polyline(&ring, false, 1.5, col_);
    }
}

/// The net wrapped over the tug, its integrity ring, and the prompt.
pub fn net_on_ship(look: &Look, w: &World, t: f32, device: Device, scale: f32) {
    let s = &w.ship;
    let Some(net) = s.net.filter(|_| s.alive) else {
        return;
    };
    let frac = (net.hp / NET_HP).clamp(0.0, 1.0) as f32;
    let (x, y) = (s.b.x as f32, s.b.y as f32);
    net_mesh(
        x,
        y,
        28.0,
        s.angle as f32 + (t * 9.0).sin() * 0.1,
        0.55 + frac * 0.4,
    );
    let top = -PI / 2.0;
    let ring: Vec<Vec2> = (0..=48)
        .map(|i| {
            let a = top + TAU * frac * i as f32 / 48.0;
            vec2(x + a.cos() * 36.0, y + a.sin() * 36.0)
        })
        .collect();
    polyline(&ring, false, 3.0, c(col::COLLECTOR));
    let keys = if device == Device::Pad {
        "STICK ◀ ▶ ◀ ▶"
    } else {
        "A ◀ D ▶ A ◀ D ▶"
    };
    let rgb = if (t * 8.0).sin() > 0.0 {
        col::COLLECTOR
    } else {
        col::TEXT
    };
    text_centered(
        &format!("NETTED · {keys}"),
        x,
        y - 50.0,
        13.0,
        &look.fonts.mono,
        c(rgb),
        scale,
    );
}

/// Draws a law unit: a patrol ship, or a bigger armoured gunship for SWAT. `t` is the animation clock in seconds.
pub fn sheriff(look: &Look, sh: &Sheriff, t: f32) {
    let chasing = sh.state == SheriffState::Pursuit;
    let xf = Xf::new(sh.b.x, sh.b.y, sh.angle);
    if sh.swat {
        // Galactic SWAT: a bigger, dark armoured gunship with red striping.
        let k = 1.25;
        let pts: Vec<(f32, f32)> = [
            (26.0, 0.0),
            (12.0, -9.0),
            (-6.0, -18.0),
            (-20.0, -14.0),
            (-22.0, 0.0),
            (-20.0, 14.0),
            (-6.0, 18.0),
            (12.0, 9.0),
        ]
        .iter()
        .map(|&(x, y)| (x * k, y * k))
        .collect();
        let hull = xf.pts(&pts);
        fill_poly(&hull, c(0x16181f));
        glow_stroke(
            &hull,
            true,
            if sh.flash > 0.0 {
                col::TEXT
            } else {
                col::DANGER
            },
            2.0,
            1.0,
        );
        fill_convex(
            &rect_pts(&xf, -14.0 * k, -2.0 * k, 24.0 * k, 4.0 * k),
            c(col::DANGER),
        );
        fill_convex(
            &rect_pts(&xf, 14.0 * k, -2.5 * k, 8.0 * k, 5.0 * k),
            c(0xffb3a8),
        );
    } else {
        let hull = xf.pts(&[
            (26.0, 0.0),
            (8.0, -10.0),
            (-14.0, -16.0),
            (-20.0, -6.0),
            (-20.0, 6.0),
            (-14.0, 16.0),
            (8.0, 10.0),
        ]);
        fill_fan(xf.o, &hull, c(0xdfe7f5));
        glow_stroke(
            &hull,
            true,
            if sh.flash > 0.0 {
                col::DANGER
            } else {
                col::LAW
            },
            2.0,
            1.0,
        );
        fill_convex(&rect_pts(&xf, -12.0, -3.0, 22.0, 6.0), c(col::LAW));
        fill_convex(&rect_pts(&xf, 4.0, -4.0, 8.0, 8.0), c(0x10182c));
    }
    // Light bar: alternating red and blue, fast in a chase.
    let phase = ((t * if chasing { 8.0 } else { 2.0 }).floor() as i64) % 2 == 1;
    let k = if sh.swat { 1.25 } else { 1.0 };
    let r = if chasing { 16.0 } else { 8.0 } * k;
    let (a, b) = (xf.p(-6.0 * k, -7.0 * k), xf.p(-6.0 * k, 7.0 * k));
    let (ca, cb) = if phase {
        (col::DANGER, col::LAW)
    } else {
        (col::LAW, col::DANGER)
    };
    look.additive(|| {
        look.glow(a.x, a.y, r, ca, true, 1.0);
        look.glow(b.x, b.y, r, cb, true, 1.0);
    });
}

/// While a sheriff closes in, a ring around the tug fills toward arrest.
pub fn cuff_meter(look: &Look, w: &World, t: f32, scale: f32) {
    let s = &w.ship;
    let Some(sh) = w
        .sheriffs
        .iter()
        .find(|x| x.state == SheriffState::Pursuit && x.cuff > 0.0)
        .filter(|_| s.alive)
    else {
        return;
    };
    let frac = (sh.cuff / sh.cuff_time).clamp(0.0, 1.0) as f32;
    let (x, y) = (s.b.x as f32, s.b.y as f32);
    let top = -PI / 2.0;
    let ring: Vec<Vec2> = (0..=48)
        .map(|i| {
            let a = top + TAU * frac * i as f32 / 48.0;
            vec2(x + a.cos() * 44.0, y + a.sin() * 44.0)
        })
        .collect();
    polyline(&ring, false, 4.0, c(col::LAW));
    let rgb = if (t * 10.0).sin() > 0.0 {
        col::LAW
    } else {
        col::DANGER
    };
    text_centered(
        "CUFFING · GET CLEAR",
        x,
        y - 58.0,
        13.0,
        &look.fonts.mono,
        c(rgb),
        scale,
    );
}

/// A mining drone, plus its beam while cutting.
pub fn miner(m: &Miner, w: &World, t: f32, rng: &mut Rng) {
    if let Some((bx, by)) = m.beam {
        let angry = m.angry_t > 0.0 && w.ship.alive && (bx, by) == (w.ship.b.x, w.ship.b.y);
        let tip = v(m.b.x + m.angle.cos() * 18.0, m.b.y + m.angle.sin() * 18.0);
        let width = 1.5 + (t * 30.0).sin().abs() * 1.5;
        glow_stroke(
            &[tip, v(bx, by)],
            false,
            if angry { col::DANGER } else { col::MINER },
            width,
            1.0,
        );
        let r = 4.0 + rng.range(0.0, 3.0) as f32;
        draw_circle(bx as f32, by as f32, r, rgba(col::TEXT, 0.8));
    }
    let xf = Xf::new(m.b.x, m.b.y, m.angle);
    let r = m.b.r as f32;
    let hex: Vec<Vec2> = (0..6)
        .map(|i| {
            let a = i as f32 / 6.0 * TAU;
            xf.p(a.cos() * r, a.sin() * r)
        })
        .collect();
    fill_fan(xf.o, &hex, c(0x0f241b));
    glow_stroke(
        &hex,
        true,
        if m.flash > 0.0 { col::TEXT } else { col::MINER },
        2.0,
        1.0,
    );
    polyline(
        &xf.pts(&[(6.0, 0.0), (18.0, 0.0)]),
        false,
        3.0,
        c(col::MINER),
    );
    let e = xf.p(-3.0, 0.0);
    draw_circle(
        e.x,
        e.y,
        4.0,
        c(if m.angry_t > 0.0 {
            col::DANGER
        } else {
            col::ORE
        }),
    );
    let stripe = Color::new(88.0 / 255.0, 224.0 / 255.0, 160.0 / 255.0, 0.45);
    polyline(&xf.pts(&[(-9.0, -6.0), (-9.0, 6.0)]), false, 1.0, stripe);
    polyline(&xf.pts(&[(-5.0, -9.0), (-5.0, 9.0)]), false, 1.0, stripe);
}

/// Draws a sentry drone, which has a hexagonal body. `t` is the animation clock in seconds.
pub fn sentry(look: &Look, d: &Sentry, t: f32) {
    let xf = Xf::new(d.x, d.y, d.angle);
    let hex: Vec<Vec2> = (0..6)
        .map(|i| {
            let a = i as f32 / 6.0 * TAU;
            xf.p(a.cos() * 8.0, a.sin() * 8.0)
        })
        .collect();
    // Rogues run red and flicker: a controller that's no longer there.
    let col_ = if d.rogue { col::DANGER } else { col::CABLE };
    fill_fan(xf.o, &hex, c(if d.rogue { 0x2a0b0b } else { 0x0d1d26 }));
    glow_stroke(
        &hex,
        true,
        if d.flash > 0.0 { col::TEXT } else { col_ },
        1.5,
        1.0,
    );
    fill_convex(&rect_pts(&xf, 4.0, -1.2, 7.0, 2.4), c(col_));
    let orbit = d.orbit as f32;
    if !d.rogue || (t * 23.0 + orbit).sin() > -0.6 {
        let p = xf.p(0.0, 0.0);
        look.additive(|| {
            look.glow(
                p.x,
                p.y,
                7.0 + (t * 6.0 + orbit).sin() * 1.5,
                col_,
                true,
                1.0,
            )
        });
    }
}

/// A small homing missile with a motor flare; torpedoes are bigger and hot.
pub fn missile(look: &Look, m: &Missile, rng: &mut Rng) {
    let k = if m.torpedo { 1.8 } else { 1.0 };
    let xf = Xf::new(m.x, m.y, m.angle);
    let sp = |x: f32, y: f32| (x * k, y * k);
    if m.torpedo {
        let p = xf.p(-4.0 * k, 0.0);
        look.additive(|| look.glow(p.x, p.y, 9.0 * k, col::CABLE, true, 1.0));
    }
    let flame = rng.range(4.0, 9.0) as f32;
    let f = xf.pts(&[sp(-3.0, -6.0), sp(-3.0 - flame, 0.0), sp(-3.0, 6.0)]);
    draw_triangle(f[0], f[1], f[2], c(col::HAZARD));
    fill_convex(
        &xf.pts(&[sp(-4.0, -2.0), sp(6.0, -2.0), sp(6.0, 2.0), sp(-4.0, 2.0)]),
        c(col::TEXT),
    );
    let nose = xf.pts(&[sp(6.0, -2.0), sp(9.0, 0.0), sp(6.0, 2.0)]);
    draw_triangle(nose[0], nose[1], nose[2], c(col::TEXT));
    fill_convex(
        &xf.pts(&[sp(-4.0, -3.5), sp(-1.0, -3.5), sp(-1.0, 3.5), sp(-4.0, 3.5)]),
        c(col::DANGER),
    );
}

/// Draw the whole opposition in the browser's order, after the rivals.
pub fn draw_all(
    look: &Look,
    w: &World,
    t: f32,
    rng: &mut Rng,
    scale: f32,
    visible: &dyn Fn(f64, f64, f64) -> bool,
) {
    for d in &w.drones {
        if visible(d.b.x, d.b.y, d.b.r) {
            drone(look, d, t);
        }
    }
    for m in &w.miners {
        if visible(m.b.x, m.b.y, m.b.r) {
            miner(m, w, t, rng);
        }
    }
    for k in &w.collectors {
        if visible(k.b.x, k.b.y, k.b.r) || k.state == CollectorState::Tow {
            collector(look, w, k, t, rng, scale);
        }
    }
    for sh in &w.sheriffs {
        if visible(sh.b.x, sh.b.y, sh.b.r) {
            sheriff(look, sh, t);
        }
    }
    for n in &w.nets {
        if visible(n.x, n.y, NET_RADIUS) {
            net_mesh(
                n.x as f32,
                n.y as f32,
                NET_RADIUS as f32,
                n.spin as f32,
                0.9,
            );
        }
    }
    for m in &w.missiles {
        if visible(m.x, m.y, 10.0) {
            missile(look, m, rng);
        }
    }
    for d in &w.sentries {
        if visible(d.x, d.y, d.r) {
            sentry(look, d, t);
        }
    }
}

/// drawHauler: a container freighter with three cargo pods, its health as
/// a tag above it (red once below 40%).
pub fn hauler(look: &Look, h: &cj_sim::field::Hauler, t: f32, scale: f32) {
    let xf = Xf::new(h.b.x, h.b.y, h.angle);
    let flash = h.flash > 0.0;
    let hull = c(if flash { 0x2a3442 } else { 0x18202c });
    fill_convex(&rect_pts(&xf, -46.0, -16.0, 70.0, 32.0), hull);
    for x in [-40.0, -22.0, -4.0] {
        fill_convex(&rect_pts(&xf, x, -22.0, 14.0, 44.0), c(0x2b3a4f));
    }
    let nose = xf.pts(&[(24.0, -14.0), (40.0, 0.0), (24.0, 14.0)]);
    fill_convex(&nose, c(0x18202c));
    glow_stroke(
        &nose,
        true,
        if flash { col::DANGER } else { col::STEEL },
        1.5,
        1.0,
    );
    let p = xf.p(-50.0, 0.0);
    look.additive(|| {
        look.glow(
            p.x,
            p.y,
            12.0 + (t * 9.0).sin() * 2.0,
            col::ENGINE,
            true,
            1.0,
        )
    });
    let pct = js_round_pct(h.hp / h.max_hp);
    let rgb = if h.hp < h.max_hp * 0.4 {
        col::DANGER
    } else {
        col::STEEL
    };
    let s = format!("HAULER · {pct}%");
    tag(look, h.b.x as f32, h.b.y as f32 - 40.0, &s, rgb, scale);
}

/// Math.max(0, Math.round(frac * 100)).
fn js_round_pct(frac: f64) -> i64 {
    (cj_sim::js::round(frac * 100.0) as i64).max(0)
}
