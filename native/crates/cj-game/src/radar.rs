//! The radar minimap and the off-screen arrows at the screen edge
//! (drawRadar and drawIndicators in the browser's rendering code), drawn
//! in screen space under the HUD while a shift is on.

use crate::look::{col, rgba, text_centered, Look};
use cj_sim::consts::{BAY_X, BAY_Y, CARRIER_X, CARRIER_Y};
use cj_sim::field::NIGHT_RADAR;
use cj_sim::items::ItemType;
use cj_sim::js;
use cj_sim::opposition::{RefineryState, SheriffState, REFINERY_HUB_R};
use cj_sim::World;
use macroquad::prelude::*;

/// The browser's hudBottom: 10 px under the right-hand meter column.
const HUD_BOTTOM: f32 = 128.0;

fn c(rgb: u32) -> Color {
    crate::look::hex(rgb)
}

/// The sheriff in pursuit blinks between danger and law colours (200 ms).
fn pursuit_blink() -> bool {
    (get_time() * 1000.0 / 200.0).floor() as i64 % 2 == 1
}

/// radarSees: on a night shift the radar only reaches NIGHT_RADAR from a
/// live tug.
fn sees(w: &World, x: f64, y: f64) -> bool {
    let s = &w.me().ship;
    !w.cond.def().night || !s.alive || js::hypot(x - s.b.x, y - s.b.y) < NIGHT_RADAR
}

/// drawRadar: the whole field, square, in the top-right corner under the HUD.
/// Online, `blips` are what the server reports beyond the view.
pub fn radar(w: &World, view: (f32, f32), blips: &[cj_net::Blip]) {
    let (vw, vh) = view;
    let size = (vw.min(vh) * 0.44).clamp(192.0, 300.0).round();
    let (x0, y0) = (vw - size - 16.0, HUD_BOTTOM);
    // The field's real span: wider than WORLD_W on a server sized for more pilots.
    let lo = w.edge_lo;
    let k = size / (w.edge_hi - lo) as f32;
    let at = |x: f64, y: f64| (x0 + (x - lo) as f32 * k, y0 + (y - lo) as f32 * k);
    draw_rectangle(
        x0,
        y0,
        size,
        size,
        Color::new(9.0 / 255.0, 13.0 / 255.0, 21.0 / 255.0, 0.75),
    );
    draw_rectangle_lines(x0, y0, size, size, 1.0, c(col::PANEL_EDGE));
    for r in w
        .rocks
        .iter()
        .filter(|r| r.size > 1 && sees(w, r.b.x, r.b.y))
    {
        let (x, y) = at(r.b.x, r.b.y);
        draw_rectangle(x, y, 1.5, 1.5, rgba(col::ROCK, 0.5));
    }
    for wr in w.wrecks.iter().filter(|wr| sees(w, wr.b.x, wr.b.y)) {
        let (x, y) = at(wr.b.x, wr.b.y);
        let rgb = if wr.locked { col::CABLE } else { col::WRECK };
        draw_rectangle(x - 2.0, y - 2.0, 4.0, 4.0, c(rgb));
    }
    // Only the pieces worth a detour show: rares, sealed pods, black boxes,
    // keycards, cores and anything worth 800 or more.
    for it in &w.items {
        if !it.worth_a_detour() || !sees(w, it.b.x, it.b.y) {
            continue;
        }
        let rgb = match it.kind {
            ItemType::Keycard => col::CABLE,
            ItemType::Core => col::DANGER,
            _ => it.tint.unwrap_or(match it.kind {
                ItemType::Reactor => col::REACTOR,
                _ if it.hidden => col::COLLECTOR,
                ItemType::Blackbox => col::HAZARD,
                _ => col::ENGINE,
            }),
        };
        let (x, y) = at(it.b.x, it.b.y);
        draw_rectangle(x - 1.0, y - 1.0, 2.5, 2.5, c(rgb));
    }
    let (cx, cy) = at(CARRIER_X - 300.0, CARRIER_Y - 50.0);
    draw_rectangle(cx, cy, 600.0 * k, 100.0 * k, c(col::HAZARD));
    if let Some(r) = &w.refinery {
        let rgb = if r.state == RefineryState::Hungry {
            col::DANGER
        } else {
            col::ORE
        };
        let (x, y) = at(r.b.x, r.b.y);
        draw_circle_lines(
            x,
            y,
            (REFINERY_HUB_R as f32 * k * 1.5).max(4.0),
            1.5,
            c(rgb),
        );
    }
    for j in w.jumpers.iter().filter(|j| sees(w, j.b.x, j.b.y)) {
        let (x, y) = at(j.b.x, j.b.y);
        draw_rectangle(x - 1.5, y - 1.5, 3.0, 3.0, c(col::JUMPER));
    }
    for d in w.drones.iter().filter(|d| sees(w, d.b.x, d.b.y)) {
        let (x, y) = at(d.b.x, d.b.y);
        draw_rectangle(x - 1.0, y - 1.0, 2.0, 2.0, c(col::DANGER));
    }
    for co in w.collectors.iter().filter(|co| sees(w, co.b.x, co.b.y)) {
        let (x, y) = at(co.b.x, co.b.y);
        draw_rectangle(x - 2.0, y - 2.0, 4.0, 4.0, c(col::COLLECTOR));
    }
    for sh in w.sheriffs.iter().filter(|sh| sees(w, sh.b.x, sh.b.y)) {
        let rgb = if sh.state == SheriffState::Pursuit && pursuit_blink() {
            col::DANGER
        } else {
            col::LAW
        };
        let (x, y) = at(sh.b.x, sh.b.y);
        draw_rectangle(x - 2.5, y - 2.5, 5.0, 5.0, c(rgb));
    }
    for m in w.miners.iter().filter(|m| sees(w, m.b.x, m.b.y)) {
        let (x, y) = at(m.b.x, m.b.y);
        draw_rectangle(x - 1.5, y - 1.5, 3.0, 3.0, c(col::MINER));
    }
    // Online: what the server reports beyond the view, by kind, in the
    // same colours and sizes as the dots above.
    for d in blips {
        use cj_net::BlipKind::*;
        let (rgb, size) = match d.kind {
            Item => (col::ENGINE, 2.5),
            Wreck => (col::WRECK, 4.0),
            Jumper => (col::JUMPER, 3.0),
            Drone => (col::DANGER, 2.0),
            Collector => (col::COLLECTOR, 4.0),
            Sheriff => (col::LAW, 5.0),
            Miner => (col::MINER, 3.0),
        };
        let (x, y) = at(f64::from(d.x), f64::from(d.y));
        draw_rectangle(x - size / 2.0, y - size / 2.0, size, size, c(rgb));
    }
    let s = &w.me().ship;
    if s.alive {
        let (x, y) = at(s.b.x, s.b.y);
        draw_circle(x, y, 2.5, c(col::TEXT));
        if w.cond.def().night {
            draw_circle_lines(x, y, NIGHT_RADAR as f32 * k, 1.0, rgba(col::CABLE, 0.35));
        }
    }
    // The camera's view as a box.
    let z = w.cam.zoom;
    let (hw, hh) = (f64::from(vw) / 2.0 / z, f64::from(vh) / 2.0 / z);
    let (bx, by) = at(w.cam.x - hw, w.cam.y - hh);
    let box_c = Color::new(221.0 / 255.0, 229.0 / 255.0, 243.0 / 255.0, 0.35);
    draw_rectangle_lines(
        bx,
        by,
        (hw * 2.0) as f32 * k,
        (hh * 2.0) as f32 * k,
        1.0,
        box_c,
    );
}

/// drawIndicators: arrows pinned 36 px inside the screen edge for the bay,
/// the refinery, nearby rivals, collectors and the law when off screen.
pub fn indicators(look: &Look, w: &World, view: (f32, f32), dpi: f32) {
    let (vw, vh) = view;
    let z = w.cam.zoom;
    let arrow = |x: f64, y: f64, rgb: u32, label: &str, inset: f32| {
        let sx = ((x - w.cam.x) * z) as f32 + vw / 2.0;
        let sy = ((y - w.cam.y) * z) as f32 + vh / 2.0;
        if sx > 0.0 && sx < vw && sy > 0.0 && sy < vh {
            return;
        }
        let (cx, cy) = (vw / 2.0, vh / 2.0);
        let a = (sy - cy).atan2(sx - cx);
        let (hw, hh) = (vw / 2.0 - 36.0, vh / 2.0 - 36.0);
        let nz = |v: f32| if v == 0.0 { 1e-6 } else { v.abs() };
        let k = (hw / nz(a.cos())).min(hh / nz(a.sin()));
        let (ax, ay) = (cx + a.cos() * k, cy + a.sin() * k);
        let (co, si) = (a.cos(), a.sin());
        let p = |lx: f32, ly: f32| vec2(ax + lx * co - ly * si, ay + lx * si + ly * co);
        draw_triangle(p(12.0, 0.0), p(-6.0, -8.0), p(-6.0, 8.0), c(rgb));
        if !label.is_empty() {
            let lx = ax - co * (26.0 + inset);
            let ly = ay - si * (22.0 + inset);
            text_centered(label, lx, ly, 11.0, &look.fonts.mono, c(rgb), dpi);
        }
    };
    let s = &w.me().ship;
    if s.alive {
        let d = js::hypot(s.b.x - BAY_X, s.b.y - BAY_Y) / 1e3;
        arrow(BAY_X, BAY_Y, col::HAZARD, &format!("BAY {d:.1}k"), 0.0);
        if let Some(r) = &w.refinery {
            let rgb = if r.state == RefineryState::Hungry {
                col::DANGER
            } else {
                col::ORE
            };
            arrow(r.b.x, r.b.y, rgb, "REFINERY", 22.0);
        }
    }
    for j in &w.jumpers {
        if (!s.alive || js::hypot(j.b.x - s.b.x, j.b.y - s.b.y) < 2600.0) && sees(w, j.b.x, j.b.y) {
            arrow(j.b.x, j.b.y, col::JUMPER, "", 0.0);
        }
    }
    for co in &w.collectors {
        if sees(w, co.b.x, co.b.y) {
            arrow(co.b.x, co.b.y, col::COLLECTOR, "", 0.0);
        }
    }
    for sh in &w.sheriffs {
        if !sees(w, sh.b.x, sh.b.y) {
            continue;
        }
        let chasing = sh.state == SheriffState::Pursuit;
        let rgb = if chasing && pursuit_blink() {
            col::DANGER
        } else {
            col::LAW
        };
        arrow(
            sh.b.x,
            sh.b.y,
            rgb,
            if chasing { "SHERIFF" } else { "" },
            44.0,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cj_sim::rng::Rng;

    /// Night shifts shorten radar range (the browser's test).
    #[test]
    fn night_shortens_radar_range() {
        let mut w = World::new_shift(Rng::seeded(1), (1280.0, 720.0));
        let (x, y) = (w.me().ship.b.x, w.me().ship.b.y);
        assert!(sees(&w, x + 3000.0, y));
        w.cond = cj_sim::field::Condition::Night;
        assert!(!sees(&w, x + 3000.0, y));
        assert!(sees(&w, x + 400.0, y));
    }
}
