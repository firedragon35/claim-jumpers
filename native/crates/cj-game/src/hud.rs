//! The in-flight HUD (the #hud panel in src/page.html, styled by
//! src/styles.css and filled by updateHud). Drawn in logical pixels.
//!
//! Milestone 1 shows what the slice has: banked and on-the-line credits,
//! the shift clock, shield, hull and winch. Wallet, contracts, wanted
//! level, heat, engine heat and weapon counters join with their systems.

use crate::controls::Device;
use crate::look::{col, text_left, text_width, Look};
use cj_sim::career::Career;
use cj_sim::consts::RIVAL_STAGES;
use cj_sim::world::fmt_credits;
use cj_sim::World;
use macroquad::prelude::*;

/// Uppercase, letter-spaced label (.label: 11px, 0.14em, muted).
fn label(look: &Look, text: &str, x: f32, y: f32, color: Color) -> f32 {
    let size = 11.0;
    let spacing = size * 0.14;
    let mut cx = x;
    for ch in text.to_uppercase().chars() {
        let s = ch.to_string();
        cx += text_left(&s, cx, y, size, &look.fonts.mono_light, color) + spacing;
    }
    cx - x
}

fn label_width(look: &Look, text: &str) -> f32 {
    let size = 11.0;
    text.to_uppercase()
        .chars()
        .map(|c| text_width(&c.to_string(), size, &look.fonts.mono_light) + size * 0.14)
        .sum()
}

/// A meter row: label column, then an 8 px bar with a 1 px edge.
fn meter(look: &Look, name: &str, x: f32, y: f32, w: f32, frac: f32, fill: u32) {
    label(look, name, x, y, Color::from_hex(col::MUTED));
    let bx = x + 52.0 + 8.0;
    let bw = w - 60.0;
    let by = y + 2.0;
    draw_rectangle(bx, by, bw, 8.0, Color::from_hex(0x141b2b));
    draw_rectangle(
        bx,
        by,
        bw * frac.clamp(0.0, 1.0),
        8.0,
        Color::from_hex(fill),
    );
    draw_rectangle_lines(bx, by, bw, 8.0, 1.0, Color::from_hex(col::PANEL_EDGE));
}

pub fn draw(look: &Look, w: &World, career: Option<&Career>, view: (f32, f32), device: Device) {
    let (vw, vh) = view;
    let (pad, top) = (16.0, 12.0);

    // Left: banked credits and what's on the line.
    label(look, "Banked", pad, top, Color::from_hex(col::MUTED));
    text_left(
        &fmt_credits(w.banked),
        pad,
        top + 16.0,
        34.0,
        &look.fonts.display,
        Color::from_hex(col::HAZARD),
    );
    // Sealed pods stay secret on the HUD too: their value shows as "+ ?".
    let chain = w
        .chain
        .iter()
        .filter_map(|id| w.item_index(*id))
        .map(|i| &w.items[i]);
    let known = chain
        .clone()
        .filter(|it| !it.hidden)
        .fold(0.0, |a, it| a + it.value);
    let secret = if chain.clone().any(|it| it.hidden) {
        " + ?"
    } else {
        ""
    };
    let line = format!("On the line {}{secret} cr", fmt_credits(known));
    let mut ly = top + 54.0;
    let online = |s: &str, y: f32, c: u32| {
        text_left(s, pad, y, 13.0, &look.fonts.mono, Color::from_hex(c));
    };
    online(&line, ly, col::CABLE);
    // Carried-over money from earlier shifts (this shift's haul is "Banked").
    if let Some(c) = career {
        ly += 18.0;
        let (s, colr) = if c.wallet < 0.0 {
            (
                format!("Wallet −{} cr overdrawn", fmt_credits(-c.wallet)),
                col::DANGER,
            )
        } else {
            (format!("Wallet {} cr", fmt_credits(c.wallet)), col::CABLE)
        };
        online(&s, ly, colr);
    }
    // (The contract line joins with contracts in milestone 5.)
    if w.mods.grinder || w.ship.slugs > 0 {
        ly += 18.0;
        online(&format!("Slugs {}", w.ship.slugs), ly, col::TEXT);
    }

    // Centre: shift and clock (red in the last 30 seconds).
    let shift = format!("Shift {} · Hollis Belt", w.shift_no);
    label(
        look,
        &shift,
        (vw - label_width(look, &shift)) / 2.0,
        top,
        Color::from_hex(col::MUTED),
    );
    // The rivals' mood once they hold a grudge, and the under-attack
    // warning, stack between the shift label and the clock. (The wanted
    // stars join this column with the law in milestone 4.)
    let mut cy = top + 16.0;
    let stage = w.rival_stage();
    if stage > 0 {
        let txt = format!("RIVALS · {}", RIVAL_STAGES[stage].name.to_uppercase());
        let tw = text_width(&txt, 11.0, &look.fonts.mono);
        text_left(
            &txt,
            (vw - tw) / 2.0,
            cy,
            11.0,
            &look.fonts.mono,
            Color::from_hex(col::JUMPER),
        );
        cy += 15.0;
    }
    if w.under_attack_t > 0.0 {
        let txt = "⚠ UNDER ATTACK";
        let tw = text_width(txt, 13.0, &look.fonts.mono);
        text_left(
            txt,
            (vw - tw) / 2.0,
            cy,
            13.0,
            &look.fonts.mono,
            Color::from_hex(col::DANGER),
        );
        cy += 17.0;
    }
    let secs = w.time_left.ceil().max(0.0) as i64;
    let clock = format!("{}:{:02}", secs / 60, secs % 60);
    let cw = text_width(&clock, 34.0, &look.fonts.display);
    let cc = if secs <= 30 { col::DANGER } else { col::HAZARD };
    text_left(
        &clock,
        (vw - cw) / 2.0,
        cy,
        34.0,
        &look.fonts.display,
        Color::from_hex(cc),
    );

    // Right: meters, 220 px wide (or 40% of a narrow window).
    let mw = 220.0f32.min(vw * 0.4);
    let mx = vw - pad - mw;
    let s = &w.ship;
    let m = &w.mods;
    meter(
        look,
        "Shield",
        mx,
        top,
        mw,
        (s.shield / m.shield_max) as f32,
        col::SHIELD,
    );
    let hull_c = if s.hp < 30.0 {
        col::DANGER
    } else {
        col::HAZARD
    };
    meter(
        look,
        "Hull",
        mx,
        top + 20.0,
        mw,
        (s.hp / m.max_hp) as f32,
        hull_c,
    );
    meter(
        look,
        "Heat",
        mx,
        top + 40.0,
        mw,
        (s.heat.clamp(0.0, 100.0) / 100.0) as f32,
        if s.overheated {
            col::DANGER
        } else {
            col::TRADEOFF
        },
    );
    // (The afterburner's Engine meter goes here in milestone 4.)
    let load = w.tow_mass();
    let frac = (load / m.winch_cap) as f32;
    meter(
        look,
        "Winch",
        mx,
        top + 60.0,
        mw,
        frac,
        if frac > 0.85 { col::JUMPER } else { col::CABLE },
    );
    let mass = if load.fract() != 0.0 {
        format!("{load:.1}")
    } else {
        format!("{load:.0}")
    };
    let lt = format!("{mass} / {} t", m.winch_cap);
    let tw = text_width(&lt, 11.0, &look.fonts.mono_light);
    text_left(
        &lt,
        vw - pad - tw,
        top + 74.0,
        11.0,
        &look.fonts.mono_light,
        Color::from_hex(col::MUTED),
    );

    // Bottom right: the controls hint for the device in use.
    let lines: &[&str] = match device {
        Device::Keyboard => &[
            "W thrust · A D turn · S brake · Space fire",
            "E hook · Q drop last · X cut all · P pause",
        ],
        Device::Pad => &[
            "L-stick turn · RT thrust · LT brake · RB fire",
            "Y hook · B drop last · LB cut all · Start pause",
        ],
    };
    for (i, l) in lines.iter().rev().enumerate() {
        let tw = text_width(l, 11.0, &look.fonts.mono_light);
        let y = vh - 16.0 - 17.6 * (i as f32 + 1.0);
        text_left(
            l,
            vw - pad - tw,
            y,
            11.0,
            &look.fonts.mono_light,
            Color::from_hex(col::MUTED),
        );
    }
}
