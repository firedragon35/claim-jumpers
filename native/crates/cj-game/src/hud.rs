//! The in-flight HUD (the #hud panel in src/page.html, styled by
//! src/styles.css and filled by updateHud). Drawn in logical pixels.

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
/// A dark translucent plate behind a block of HUD text at (x, y), w by h,
/// with a little margin, so the text stays readable over the field.
fn plate(x: f32, y: f32, w: f32, h: f32) {
    let m = 6.0;
    draw_rectangle(
        x - m,
        y - m,
        w + 2.0 * m,
        h + 2.0 * m,
        Color::new(0.02, 0.03, 0.06, 0.55),
    );
}

fn meter(look: &Look, name: &str, x: f32, y: f32, w: f32, frac: f32, fill: u32) {
    label(look, name, x, y, crate::look::hex(col::MUTED));
    let bx = x + 52.0 + 8.0;
    let bw = w - 60.0;
    let by = y + 2.0;
    draw_rectangle(bx, by, bw, 8.0, crate::look::hex(0x141b2b));
    draw_rectangle(
        bx,
        by,
        bw * frac.clamp(0.0, 1.0),
        8.0,
        crate::look::hex(fill),
    );
    draw_rectangle_lines(bx, by, bw, 8.0, 1.0, crate::look::hex(col::PANEL_EDGE));
}

/// The banked total as shown: it counts up to the real total over about
/// half a second, so a delivery ticks in; a drop (a new shift) shows at once.
pub fn count_up(shown: f64, real: f64, dt: f64) -> f64 {
    if real <= shown || real - shown < 1.0 {
        return real;
    }
    (shown + (real - shown) * (dt * 8.0).min(1.0))
        .max(shown + 1.0)
        .min(real)
}

/// `banked` is the total to show (see [`count_up`]).
pub fn draw(
    look: &Look,
    w: &World,
    career: Option<&Career>,
    view: (f32, f32),
    device: Device,
    banked: f64,
) {
    let (vw, vh) = view;
    let (pad, top) = (16.0, 12.0);

    // Left: banked credits and what's on the line. The text lines are
    // worked out first so a backing plate can be sized to them and drawn
    // underneath: otherwise world tags and bright rocks run through them.
    // Sealed pods stay secret on the HUD too: their value shows as "+ ?".
    let chain = w
        .me()
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
    let mut lines = vec![(
        format!("On the line {}{secret} cr", fmt_credits(known)),
        col::CABLE,
    )];
    // Carried-over money from earlier shifts (this shift's haul is "Banked").
    if let Some(c) = career {
        lines.push(if c.wallet < 0.0 {
            (
                format!("Wallet −{} cr overdrawn", fmt_credits(-c.wallet)),
                col::DANGER,
            )
        } else {
            (format!("Wallet {} cr", fmt_credits(c.wallet)), col::CABLE)
        });
    }
    // The accepted contract and its progress; cable once it is met.
    if let Some(k) = &w.me().contract {
        let prog = k.n.min(w.contract_progress(0, k));
        let money = matches!(
            k.kind,
            cj_sim::field::ContractKind::Bank | cj_sim::field::ContractKind::Clean
        );
        let amount = if money {
            format!("{} / {}", fmt_credits(prog), fmt_credits(k.n))
        } else {
            format!("{prog} / {}", k.n)
        };
        let failed = k.kind == cj_sim::field::ContractKind::Clean && w.me().stats.deaths > 0;
        lines.push(if failed {
            (format!("Contract failed · {}", k.text()), col::HAZARD)
        } else if prog >= k.n {
            (format!("Contract · {} · {amount}", k.text()), col::CABLE)
        } else {
            (format!("Contract · {} · {amount}", k.text()), col::HAZARD)
        });
    }
    // The short gear rows below (slugs, missiles, sentries) need no width.
    let gear = [
        w.me().mods.grinder || w.me().ship.slugs > 0,
        w.me().mods.missiles,
        w.me().mods.drone_bay,
    ]
    .iter()
    .filter(|b| **b)
    .count();
    let widest = lines
        .iter()
        .map(|(s, _)| text_width(s, 13.0, &look.fonts.mono))
        .fold(150.0f32, f32::max);
    let rows = (lines.len() + gear) as f32;
    plate(pad, top, widest, 54.0 + 18.0 * (rows - 1.0) + 16.0);
    label(look, "Banked", pad, top, crate::look::hex(col::MUTED));
    text_left(
        &fmt_credits(banked.round()),
        pad,
        top + 16.0,
        34.0,
        &look.fonts.display,
        crate::look::hex(col::HAZARD),
    );
    let mut ly = top + 54.0 - 18.0;
    let online = |s: &str, y: f32, c: u32| {
        text_left(s, pad, y, 13.0, &look.fonts.mono, crate::look::hex(c));
    };
    for (s, c) in &lines {
        ly += 18.0;
        online(s, ly, *c);
    }
    if w.me().mods.grinder || w.me().ship.slugs > 0 {
        ly += 18.0;
        online(&format!("Slugs {}", w.me().ship.slugs), ly, col::TEXT);
    }
    if w.me().mods.missiles {
        ly += 18.0;
        let lw = text_left(
            "Missiles ",
            pad,
            ly,
            13.0,
            &look.fonts.mono,
            crate::look::hex(col::HAZARD),
        );
        let rack = w.me().mods.missile_rack as usize;
        let n = (w.me().ship.missiles as usize).min(rack);
        crate::ui::pips(
            pad + lw,
            ly + 8.0,
            n,
            rack,
            13.0,
            crate::look::hex(col::HAZARD),
        );
    }
    if w.me().mods.drone_bay {
        ly += 18.0;
        let alive = w.sentries.iter().filter(|d| !d.dead && !d.rogue).count();
        let max = cj_sim::parts::SENTRY_MAX as usize;
        let s = format!(
            "Sentries {}{}",
            "◆".repeat(alive),
            "◇".repeat(max.saturating_sub(alive))
        );
        online(&s, ly, col::CABLE);
    }

    // Centre: shift and clock (red in the last 30 seconds).
    let cond = if w.cond == cj_sim::field::Condition::Normal {
        String::new()
    } else {
        format!(" · {}", w.cond.def().name)
    };
    let shift = format!("Shift {} · {}{cond}", w.shift_no, w.belt.def().name);
    label(
        look,
        &shift,
        (vw - label_width(look, &shift)) / 2.0,
        top,
        crate::look::hex(col::MUTED),
    );
    // The rivals' mood once they hold a grudge, and the under-attack
    // warning, stack between the shift label and the clock.
    let mut cy = top + 16.0;
    // Wanted stars: always shown, dim when clean; the law tier that's
    // coming for you once wanted; flashing while anyone is in pursuit.
    {
        use cj_sim::opposition::{law_tier, SheriffState, WANTED_MAX};
        let tier = law_tier(w.me().wanted);
        let note = match tier {
            Some(t) => format!(" · {}", t.name.to_uppercase()),
            None if w.sheriffs.iter().any(|s| s.shakedown) => " · SHAKEDOWN".to_owned(),
            None => String::new(),
        };
        let pursuit = w.sheriffs.iter().any(|s| s.state == SheriffState::Pursuit);
        let flash = pursuit && (get_time() * 4.0).floor() as i64 % 2 == 1;
        let color = if w.me().wanted == 0 {
            crate::look::rgba(col::MUTED, 0.55)
        } else if tier.is_some_and(|t| t.swat) || flash {
            crate::look::hex(col::DANGER)
        } else {
            crate::look::hex(col::LAW)
        };
        let font = if w.me().wanted == 0 {
            &look.fonts.mono_light
        } else {
            &look.fonts.mono
        };
        let head = "WANTED ";
        let star_w = 13.0;
        let total = text_width(head, 13.0, font)
            + star_w * WANTED_MAX as f32
            + text_width(&note, 13.0, font);
        let mut x = (vw - total) / 2.0;
        x += text_left(head, x, cy, 13.0, font, color);
        for i in 0..WANTED_MAX {
            crate::look::star(x + star_w / 2.0, cy + 7.0, 5.5, i < w.me().wanted, color);
            x += star_w;
        }
        text_left(&note, x, cy, 13.0, font, color);
        cy += 17.0;
    }
    // Online, an impounded tug sits out the rest of the shift while the
    // others fly on; say so, since the field shows no tug and no respawn.
    if w.me().impounded && !w.shift_over {
        let line = "IMPOUNDED · BACK NEXT SHIFT";
        let font = &look.fonts.mono;
        let color = crate::look::hex(col::DANGER);
        text_left(
            line,
            (vw - text_width(line, 13.0, font)) / 2.0,
            cy,
            13.0,
            font,
            color,
        );
        cy += 17.0;
    }
    // PvP: arming, on (with any combat lock), or attacked by a PvP pilot
    // you may fire back at. Nothing while Protected and left alone.
    if let Some(line) = pvp_line(w.me()) {
        let font = &look.fonts.mono;
        let color = crate::look::hex(col::DANGER);
        text_left(
            &line,
            (vw - text_width(&line, 13.0, font)) / 2.0,
            cy,
            13.0,
            font,
            color,
        );
        cy += 17.0;
    }
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
            crate::look::hex(col::JUMPER),
        );
        cy += 15.0;
    }
    if w.me().under_attack_t > 0.0 {
        let txt = "⚠ UNDER ATTACK";
        let tw = text_width(txt, 13.0, &look.fonts.mono);
        text_left(
            txt,
            (vw - tw) / 2.0,
            cy,
            13.0,
            &look.fonts.mono,
            crate::look::hex(col::DANGER),
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
        crate::look::hex(cc),
    );

    // Right: meters, 220 px wide (or 40% of a narrow window).
    let mw = 220.0f32.min(vw * 0.4);
    let mx = vw - pad - mw;
    let s = &w.me().ship;
    let m = &w.me().mods;
    plate(mx, top, mw, 94.0 + 12.0);
    meter(
        look,
        "Shield",
        mx,
        top,
        mw,
        (s.shield / m.shield_max) as f32,
        col::SHIELD,
    );
    // The same 30 % line as the hull alarm.
    let hull_c = if s.hp < m.max_hp * 0.3 {
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
    meter(
        look,
        "Engine",
        mx,
        top + 60.0,
        mw,
        (s.engine_heat.clamp(0.0, 100.0) / 100.0) as f32,
        // The browser styles no "hot" state for this bar: always amber.
        col::HAZARD,
    );
    let load = w.tow_mass(0);
    let frac = (load / m.winch_cap) as f32;
    meter(
        look,
        "Winch",
        mx,
        top + 80.0,
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
        top + 94.0,
        11.0,
        &look.fonts.mono_light,
        crate::look::hex(col::MUTED),
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
    let hint_w = lines
        .iter()
        .map(|l| text_width(l, 11.0, &look.fonts.mono_light))
        .fold(0.0f32, f32::max);
    let hint_top = vh - 16.0 - 17.6 * lines.len() as f32;
    plate(
        vw - pad - hint_w,
        hint_top,
        hint_w,
        17.6 * lines.len() as f32,
    );
    for (i, l) in lines.iter().rev().enumerate() {
        let tw = text_width(l, 11.0, &look.fonts.mono_light);
        let y = vh - 16.0 - 17.6 * (i as f32 + 1.0);
        text_left(
            l,
            vw - pad - tw,
            y,
            11.0,
            &look.fonts.mono_light,
            crate::look::hex(col::MUTED),
        );
    }
}

/// Online: a small badge under the meters saying so, with how many pilots
/// are flying and the round trip, so a laggy shift explains itself.
pub fn online_badge(look: &Look, view: (f32, f32), pilots: usize, rtt_ms: f64) {
    let s = format!(
        "ONLINE · {pilots} pilot{} · {rtt_ms:.0} ms",
        if pilots == 1 { "" } else { "s" }
    );
    let tw = text_width(&s, 11.0, &look.fonts.mono);
    // Under the winch load line (top + 94), clear of the radar below it.
    let (x, y) = (view.0 - 16.0 - tw, 12.0 + 112.0);
    plate(x, y, tw, 12.0);
    let c = if rtt_ms > 150.0 {
        col::HAZARD
    } else {
        col::CABLE
    };
    text_left(&s, x, y, 11.0, &look.fonts.mono, crate::look::hex(c));
}

/// The HUD's PvP status for pilot `me`, if there is anything to say.
/// Seconds are rounded up, so a countdown never shows 0 while running.
fn pvp_line(me: &cj_sim::world::Pilot) -> Option<String> {
    use cj_sim::perm::Pvp;
    let secs = |t: f64| t.ceil() as u32;
    match me.pvp {
        Pvp::Arming { t } => Some(format!("PVP ARMING · {}", secs(t))),
        Pvp::Enabled if me.combat_t > 0.0 => {
            Some(format!("PVP ON · COMBAT LOCK {}", secs(me.combat_t)))
        }
        Pvp::Enabled => Some("PVP ON".to_owned()),
        Pvp::Protected => me
            .payback
            .iter()
            .map(|&(_, t)| t)
            .reduce(f64::max)
            .map(|t| format!("ATTACKED · FIRE BACK {}", secs(t))),
    }
}

#[cfg(test)]
mod pvp_tests {
    use super::pvp_line;
    use cj_sim::mods::Mods;
    use cj_sim::perm::Pvp;
    use cj_sim::world::Pilot;

    /// Each PvP state has its line, and a quiet Protected pilot has none.
    #[test]
    fn the_pvp_line_follows_the_state() {
        let mut p = Pilot::new(Mods::stock());
        assert_eq!(pvp_line(&p), None);
        p.payback.push((1, 12.2));
        assert_eq!(pvp_line(&p).as_deref(), Some("ATTACKED · FIRE BACK 13"));
        p.pvp = Pvp::Arming { t: 9.5 };
        assert_eq!(pvp_line(&p).as_deref(), Some("PVP ARMING · 10"));
        p.pvp = Pvp::Enabled;
        assert_eq!(pvp_line(&p).as_deref(), Some("PVP ON"));
        p.combat_t = 0.4;
        assert_eq!(pvp_line(&p).as_deref(), Some("PVP ON · COMBAT LOCK 1"));
    }
}

#[cfg(test)]
mod tests {
    use super::count_up;

    /// A delivery ticks up to the new total within about a second; a lower
    /// total (a new shift) shows at once.
    #[test]
    fn the_banked_total_counts_up_and_drops_at_once() {
        let mut shown = 0.0;
        let mut steps = 0;
        while shown < 1000.0 {
            let next = count_up(shown, 1000.0, 1.0 / 60.0);
            assert!(next > shown && next <= 1000.0);
            shown = next;
            steps += 1;
        }
        assert!((10..=60).contains(&steps), "{steps}");
        assert_eq!(count_up(1000.0, 0.0, 1.0 / 60.0), 0.0);
        assert_eq!(count_up(999.5, 1000.0, 1.0 / 60.0), 1000.0);
    }
}
