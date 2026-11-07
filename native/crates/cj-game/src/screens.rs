//! The menu screens: title, pause, shift report and machine shop (the
//! #screen-* cards in src/page.html, filled by src/js/18-hud-and-screens.js).
//! Each draws itself through the `ui` kit and returns what the player
//! chose; main.rs carries the choice out.

use crate::look::{col, rgba, Look};
use crate::tug::{self, Frame, TugState};
use crate::ui::{
    self, card, label, paragraph, paragraph_h, text, text_w, MenuInput, Style, Ui, Which,
};
use cj_sim::career::{rank_for, Career, Mode, Outcome, Settlement, MODES, REP_NAMES};
use cj_sim::parts::*;
use cj_sim::rng::Rng;
use cj_sim::world::{fmt_credits, World};
use macroquad::prelude::*;

/// What the player picked on a screen this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Act {
    StartOrContinue,
    NewCareer,
    PickMode(Mode),
    Quit,
    Resume,
    EndShiftNow,
    PauseToMenu,
    ReportNext,
    ToMenu,
    StartShift,
    Pay(f64),
    Buy(Part),
    Mount(Part),
    Unmount(Part),
    Level(System),
}

fn c(rgb: u32) -> Color {
    Color::from_hex(rgb)
}

fn cr(n: f64) -> String {
    format!("{} cr", fmt_credits(n))
}

/// Negative credits with a true minus sign, as the browser writes them.
fn neg(n: f64) -> String {
    format!("\u{2212}{}", cr(n))
}

/* ------------------------------- Title -------------------------------- */

pub struct TitleView<'a> {
    pub saved: Option<&'a Career>,
    pub mode: Mode,
    pub best: Option<f64>,
    /// Field-guide tab (0 keyboard, 1 gamepad, 2 bay rates, 3 notes).
    pub tab: &'a mut usize,
    /// "Erase saved career? Press again" is showing.
    pub armed_new: bool,
}

const GUIDE_TABS: [&str; 4] = ["Keyboard", "Gamepad", "Bay rates", "Field notes"];

pub fn title(
    look: &Look,
    u: &mut Ui,
    input: &MenuInput,
    view: (f32, f32),
    dpi: f32,
    v: TitleView,
) -> Option<Act> {
    let (w, h) = (1200.0, 540.0);
    u.begin(input, (w, h), view, dpi);
    if input.tab != 0 {
        *v.tab = (*v.tab as i32 + input.tab).rem_euclid(4) as usize;
    }
    let r = Rect::new(20.0, 20.0, 1160.0, 500.0);
    card(r);
    let mut act = None;
    let (x, mut y) = (r.x + 36.0, r.y + 34.0);
    label(
        look,
        u,
        "Hollis Belt Salvage · Tug KM-7 \"Second Mortgage\"",
        x,
        y,
        c(col::MUTED),
    );
    y += 22.0;
    text(
        look,
        u,
        "Claim Jumpers",
        x,
        y,
        96.0,
        Which::Display,
        c(col::HAZARD),
    );
    y += 100.0;
    let lede = "Each shift is five minutes. Break rocks, crack wrecks, and tow anything valuable into the carrier bay. Everything on your cable makes the tug heavier, and rival crews want it more than you do. After every shift the bank takes a loan installment. Spend what's left on upgrades, and don't miss a payment.";
    y += paragraph(
        look,
        u,
        lede,
        x,
        y,
        560.0,
        18.0,
        Which::Body,
        c(col::TEXT),
        1.45,
    ) + 22.0;

    label(
        look,
        u,
        if v.saved.is_some() {
            "Difficulty for a new career"
        } else {
            "Difficulty"
        },
        x,
        y,
        c(col::MUTED),
    );
    y += 20.0;
    let bw = 560.0 / 3.0;
    for (i, m) in MODES.into_iter().enumerate() {
        let br = Rect::new(x + i as f32 * bw, y, bw, 38.0);
        if u.button(
            look,
            &format!("mode-{i}"),
            br,
            Style::Mode {
                pressed: m == v.mode,
            },
            m.def().label,
            true,
        ) {
            act = Some(Act::PickMode(m));
        }
    }
    y += 46.0;
    y += paragraph(
        look,
        u,
        v.mode.def().blurb,
        x,
        y,
        560.0,
        12.0,
        Which::MonoLight,
        c(col::MUTED),
        1.4,
    ) + 18.0;

    let start = match v.saved {
        Some(s) => format!("Continue · shift {} · {}", s.shift, s.mode.def().label),
        None => "Start career".to_owned(),
    };
    let (p, sr) = u.button_at(look, "start", x, y, Style::Primary, &start, true);
    if p {
        act = Some(Act::StartOrContinue);
    }
    let mut bx = sr.x + sr.w + 12.0;
    let by = y + (sr.h - 37.0) / 2.0;
    if v.saved.is_some() {
        let lbl = if v.armed_new {
            "Erase saved career? Press again".to_owned()
        } else {
            format!("New {} career", v.mode.def().label)
        };
        let (p, nr) = u.button_at(look, "new", bx, by, Style::Ghost, &lbl, true);
        if p {
            act = Some(Act::NewCareer);
        }
        bx = nr.x + nr.w + 12.0;
    }
    // A desktop game needs a way out besides the window's close button.
    if u.button_at(look, "quit", bx, by, Style::Ghost, "Quit", true)
        .0
    {
        act = Some(Act::Quit);
    }
    y += sr.h + 12.0;
    if let Some(b) = v.best {
        let s = format!("Best {} shift: {}", v.mode.def().label, cr(b));
        text(look, u, &s, x, y, 12.0, Which::MonoLight, c(col::MUTED));
    }

    // Field guide.
    let gx = r.x + 640.0;
    let gw = r.x + r.w - 36.0 - gx;
    let mut tx = gx;
    let ty = r.y + 34.0;
    for (i, name) in GUIDE_TABS.iter().enumerate() {
        let rr = ui::button_rect(look, tx, ty, Style::Tab { selected: false }, name);
        if u.button(
            look,
            &format!("guide-{i}"),
            rr,
            Style::Tab {
                selected: *v.tab == i,
            },
            name,
            true,
        ) {
            *v.tab = i;
        }
        tx += rr.w;
    }
    let py = ty + 33.0;
    draw_line(gx, py, gx + gw, py, 1.0, c(col::PANEL_EDGE));
    guide(look, u, *v.tab, gx, py + 14.0, gw);
    act
}

/// The field guide's panels (#guide-keys, #guide-pad, #guide-rates, #guide-notes).
fn guide(look: &Look, u: &Ui, tab: usize, x: f32, y: f32, w: f32) {
    let rows: &[(&str, &str)] = match tab {
        0 => &[
            ("W / ↑", "Thrust"),
            ("A D / ← →", "Turn"),
            ("S / ↓", "Brake"),
            ("Space", "Cutting cannon"),
            ("E", "Hook nearby salvage"),
            ("Q", "Drop last piece (fling it)"),
            ("X", "Cut the whole line"),
            ("Z / C · G", "Strafe · grind (with upgrades)"),
            ("M", "Mute all"),
        ],
        1 => &[
            ("L-stick / D-pad", "Turn"),
            ("RT or A", "Thrust (RT is analog)"),
            ("LT", "Brake"),
            ("RB", "Cutting cannon"),
            ("Y · B · LB", "Hook · drop last · cut all"),
            ("R-stick · X", "Strafe · grind (with upgrades)"),
            ("Start", "Start shift / pause"),
        ],
        2 => &[
            ("Scrap", "50–160 cr · 1 t"),
            ("Ore", "110–450 cr · 1.5 t"),
            ("Plating", "180–420 cr · 3–3.9 t"),
            ("Engines", "550–1,500 cr · 7–8 t"),
            ("Reactor cores", "900–2,200 cr · 4 t · volatile"),
            ("Flight recorder", "950 cr · 0.5 t · rare"),
            ("Cargo pod", "200–2,000 cr · revealed in the bay"),
        ],
        _ => &[],
    };
    if tab == 3 {
        let notes = "Swing cargo into jumpers to wreck them. Yank the line to shake off crabs, and spin the tug to throw them off the hull. Scrap too many rival jumpers and the crews hold a grudge. Reactors blow if you hit them hard or shoot them. Your shield soaks hits but takes a while to come back. Parking in the bay repairs you and recharges it faster.";
        paragraph(
            look,
            u,
            notes,
            x,
            y,
            w,
            12.0,
            Which::MonoLight,
            c(col::MUTED),
            1.5,
        );
        return;
    }
    let dw = rows
        .iter()
        .map(|r| text_w(look, r.0, 13.0, Which::MonoLight))
        .fold(0.0, f32::max);
    for (i, (dt, dd)) in rows.iter().enumerate() {
        let ry = y + i as f32 * 23.0;
        text(
            look,
            u,
            dt,
            x,
            ry + 1.0,
            13.0,
            Which::MonoLight,
            c(col::TEXT),
        );
        text(
            look,
            u,
            dd,
            x + dw + 12.0,
            ry,
            15.0,
            Which::Body,
            c(col::MUTED),
        );
    }
    if tab == 2 {
        let ny = y + rows.len() as f32 * 23.0 + 10.0;
        let s = "Every piece rolls a grade. Bright, glinting ones are the rare grades.";
        paragraph(
            look,
            u,
            s,
            x,
            ny,
            w,
            12.0,
            Which::MonoLight,
            c(col::MUTED),
            1.4,
        );
    }
}

/* ------------------------------- Pause -------------------------------- */

pub fn pause(
    look: &Look,
    u: &mut Ui,
    input: &MenuInput,
    view: (f32, f32),
    dpi: f32,
    armed_menu: bool,
) -> Option<Act> {
    let (w, h) = (680.0, 300.0);
    u.begin(input, (w, h), view, dpi);
    let r = Rect::new(20.0, 20.0, 640.0, 260.0);
    card(r);
    let (x, mut y) = (r.x + 36.0, r.y + 34.0);
    label(look, u, "Shift paused", x, y, c(col::MUTED));
    y += 22.0;
    text(
        look,
        u,
        "On break",
        x,
        y,
        64.0,
        Which::Display,
        c(col::HAZARD),
    );
    y += 72.0;
    text(
        look,
        u,
        "The meter isn't running. Marge is.",
        x,
        y,
        18.0,
        Which::Body,
        c(col::TEXT),
    );
    y += 44.0;
    let mut act = None;
    let (p, rr) = u.button_at(look, "resume", x, y, Style::Primary, "Resume", true);
    if p || input.back {
        act = Some(Act::Resume);
    }
    let by = y + (rr.h - 37.0) / 2.0;
    let (p, er) = u.button_at(
        look,
        "end",
        rr.x + rr.w + 12.0,
        by,
        Style::Ghost,
        "End shift now",
        true,
    );
    if p {
        act = Some(Act::EndShiftNow);
    }
    let menu = if armed_menu {
        "Ends this shift. Press again"
    } else {
        "Main menu"
    };
    if u.button_at(
        look,
        "menu",
        er.x + er.w + 12.0,
        by,
        Style::Ghost,
        menu,
        true,
    )
    .0
    {
        act = Some(Act::PauseToMenu);
    }
    act
}

/* ---------------------------- Shift report ---------------------------- */

/// What the report shows, kept from the moment the shift ended.
pub struct Report {
    pub settlement: Settlement,
    pub world_items: u32,
    pub best_run: f64,
    pub jumpers: u32,
    pub lost: f64,
    pub deaths: u32,
    pub bounties: f64,
    pub kills: [(u32, &'static str, &'static str); 4],
    /// The career after settling (for the wallet, loan and notes).
    pub career: Career,
    pub mode: Mode,
    pub best: f64,
    pub is_best: bool,
}

impl Report {
    pub fn new(st: Settlement, w: &World, career: &Career, best: f64, is_best: bool) -> Report {
        let k = &w.kills;
        Report {
            settlement: st,
            world_items: w.stats.items,
            best_run: w.stats.best_run,
            jumpers: w.stats.jumpers,
            lost: w.stats.lost,
            deaths: w.stats.deaths,
            bounties: w.bounties,
            kills: [
                (k.skiff, "skiffs", "skiff"),
                (k.cutter, "cutters", "cutter"),
                (k.brute, "brutes", "brute"),
                (k.gunship, "gunships", "gunship"),
            ],
            career: career.clone(),
            mode: career.mode,
            best,
            is_best,
        }
    }

    /// Title, rank, note and main-button label (endShift and showPaidOff).
    fn words(&self) -> (String, String, String, &'static str) {
        let c = &self.career;
        let earned = self.settlement.earned;
        let rank = rank_for(earned).to_owned();
        match &self.settlement.outcome {
            Outcome::Continue => {
                let shift = c.shift - 1;
                let heat = if c.grudge >= 2.0 {
                    let st = cj_sim::consts::RIVAL_STAGES
                        [cj_sim::consts::rival_stage_index(c.grudge)]
                    .name;
                    format!(" Rivals still {} next shift.", st.to_lowercase())
                } else {
                    String::new()
                };
                let limit = c.overdraft_limit();
                let overdraft = if c.wallet < 0.0 {
                    format!(
                        " You're {} overdrawn; the bank repossesses past {}.",
                        cr(-c.wallet),
                        cr(limit)
                    )
                } else {
                    String::new()
                };
                let title = if self.is_best && earned > 0.0 {
                    "New personal best".to_owned()
                } else {
                    format!("Shift {shift} complete")
                };
                let note = format!(
                    "Next installment is {}, due after shift {}.{overdraft}{heat} Marge suggests not spending all of it.",
                    cr(c.due(0)),
                    c.shift
                );
                (title, rank, note, "Outfit the tug")
            }
            Outcome::Repossessed { over } => {
                let shift = c.shift;
                let note = format!(
                    "You ended up {} overdrawn, {} past the bank's limit. They took the \"{}\" after {} shift{}. Marge kept your mug.",
                    cr(-c.wallet),
                    cr(*over),
                    c.tug().name,
                    shift,
                    if shift > 1 { "s" } else { "" }
                );
                ("Repossessed".to_owned(), rank, note, "Start a new career")
            }
            Outcome::TradedUp { from, rolled } => {
                let paid = &TUG_CLASSES[*from];
                let next = c.tug();
                let roll = if *rolled > 0.0 {
                    format!(", including your {} overdraft", cr(*rolled))
                } else {
                    String::new()
                };
                let note = format!(
                    "You owned the \"{}\" outright for about a minute. Kessler Mutual traded it in for the {} \"{}\": +{} hull, +{} shield, stronger engines, +{} t on the winch. Your parts and systems came across. New loan: {}{roll}. Marge says the paint job is \"a choice.\"",
                    paid.name, next.mk, next.name, next.hp, next.shield, next.winch, cr(c.loan)
                );
                (
                    format!("Traded up: {} \"{}\"", next.mk, next.name),
                    "Owner-operator, briefly".to_owned(),
                    note,
                    "Outfit the new tug",
                )
            }
            Outcome::FleetPaid => {
                let paid = c.tug();
                // The browser counts this after the shift number has moved on.
                let shift = c.shift;
                let note = format!(
                    "You own the {} \"{}\" outright after {} shift{} and {} hauled. Every class, every loan, paid. Marge is checking the books for fraud.",
                    paid.mk,
                    paid.name,
                    shift,
                    if shift > 1 { "s" } else { "" },
                    cr(c.total_banked)
                );
                (
                    "Fleet paid off".to_owned(),
                    "Owner-operator".to_owned(),
                    note,
                    "Start a new career",
                )
            }
        }
    }
}

pub fn report(
    look: &Look,
    u: &mut Ui,
    input: &MenuInput,
    view: (f32, f32),
    dpi: f32,
    rep: &Report,
) -> Option<Act> {
    let (w, h) = (1040.0, 580.0);
    u.begin(input, (w, h), view, dpi);
    let r = Rect::new(20.0, 20.0, 1000.0, 540.0);
    card(r);
    let (title, rank, note, primary) = rep.words();
    let st = &rep.settlement;
    let c = &rep.career;
    let (x, mut y) = (r.x + 36.0, r.y + 34.0);
    label(look, u, "Shift report", x, y, self::c(col::MUTED));
    y += 22.0;
    // Long trade-up titles shrink to stay on one line.
    let size = if text_w(look, &title, 64.0, Which::Display) > 560.0 {
        44.0
    } else {
        64.0
    };
    text(
        look,
        u,
        &title,
        x,
        y,
        size,
        Which::Display,
        self::c(col::HAZARD),
    );
    y += size + 8.0;
    text(
        look,
        u,
        &rank,
        x,
        y,
        28.0,
        Which::Display,
        self::c(col::CABLE),
    );
    y += 44.0;

    // Stats: three across, two down.
    let stats = [
        ("Banked", cr(st.earned)),
        ("Pieces", fmt_credits(f64::from(rep.world_items))),
        ("Best delivery", cr(rep.best_run)),
        ("Jumpers scrapped", fmt_credits(f64::from(rep.jumpers))),
        ("Stolen / lost", cr(rep.lost)),
        ("Tugs wrecked", fmt_credits(f64::from(rep.deaths))),
    ];
    for (i, (l, v)) in stats.iter().enumerate() {
        let sx = x + (i % 3) as f32 * 190.0;
        let sy = y + (i / 3) as f32 * 66.0;
        draw_line(sx, sy, sx + 178.0, sy, 1.0, self::c(col::PANEL_EDGE));
        label(look, u, l, sx, sy + 8.0, self::c(col::MUTED));
        text(
            look,
            u,
            v,
            sx,
            sy + 24.0,
            30.0,
            Which::Display,
            self::c(col::TEXT),
        );
    }
    y += 140.0;

    // The ledger.
    let kills: Vec<String> = rep
        .kills
        .iter()
        .filter(|k| k.0 > 0)
        .map(|k| format!("{} {}", k.0, if k.0 == 1 { k.2 } else { k.1 }))
        .collect();
    let rivals: u32 = rep.kills.iter().map(|k| k.0).sum();
    let sign = |n: f64| {
        if n > 0.0 {
            format!("+{n}")
        } else if n < 0.0 {
            format!("\u{2212}{}", -n)
        } else {
            "±0".to_owned()
        }
    };
    let standing = (0..3)
        .map(|i| format!("{} {}", REP_NAMES[i], sign(st.rep_delta[i])))
        .collect::<Vec<_>>()
        .join(" · ");
    let ended = !matches!(st.outcome, Outcome::Continue | Outcome::TradedUp { .. });
    let wallet = if c.wallet < 0.0 {
        format!(
            "{} (overdraft, limit {})",
            neg(-c.wallet),
            cr(c.overdraft_limit())
        )
    } else {
        cr(c.wallet)
    };
    let loan_left = if matches!(st.outcome, Outcome::TradedUp { .. }) {
        c.loan
    } else {
        c.loan.max(0.0)
    };
    let rows: Vec<(&str, String, u32)> = vec![
        ("Shift earnings", format!("+{}", cr(st.earned)), col::TEXT),
        (
            "Rival bounties",
            if rep.bounties > 0.0 {
                format!(
                    "+{} · {rivals} rival{} (in earnings)",
                    cr(rep.bounties),
                    if rivals == 1 { "" } else { "s" }
                )
            } else {
                "None".to_owned()
            },
            col::TEXT,
        ),
        (
            "Kills",
            if kills.is_empty() {
                "None".to_owned()
            } else {
                kills.join(", ")
            },
            col::TEXT,
        ),
        // (Contracts and crew wages arrive in milestone 5.)
        ("Contract", "None".to_owned(), col::TEXT),
        ("Standing", standing, col::TEXT),
        ("Crew wages", "None".to_owned(), col::TEXT),
        ("Loan installment", neg(st.due), col::DANGER),
        (
            "Wallet",
            wallet,
            if c.wallet < 0.0 {
                col::DANGER
            } else {
                col::TEXT
            },
        ),
        (
            "Loan remaining",
            cr(if ended && c.loan <= 0.0 {
                0.0
            } else {
                loan_left
            }),
            col::TEXT,
        ),
    ];
    ui::ledger(look, u, x, y, &rows);

    // The side column: the note and the buttons.
    let sx = r.x + 640.0;
    let sw = r.x + r.w - 36.0 - sx;
    let mut sy = r.y + 90.0;
    sy += paragraph(
        look,
        u,
        &note,
        sx,
        sy,
        sw,
        18.0,
        Which::Body,
        self::c(col::TEXT),
        1.45,
    ) + 24.0;
    let mut act = None;
    let (p, pr) = u.button_at(look, "again", sx, sy, Style::Primary, primary, true);
    if p {
        act = Some(Act::ReportNext);
    }
    if u.button_at(
        look,
        "menu",
        sx,
        sy + pr.h + 12.0,
        Style::Ghost,
        "Main menu",
        true,
    )
    .0
    {
        act = Some(Act::ToMenu);
    }
    let best = format!(
        "Best single shift ({}): {}",
        rep.mode.def().label,
        cr(rep.best)
    );
    text(
        look,
        u,
        &best,
        sx,
        sy + pr.h + 66.0,
        12.0,
        Which::MonoLight,
        self::c(col::MUTED),
    );
    act
}

/* ---------------------------- Machine shop ---------------------------- */

pub struct ShopView<'a> {
    pub career: &'a Career,
    /// 0 = Tug, 1 = Parts.
    pub tab: &'a mut usize,
    pub msg: &'a str,
    pub t: f32,
}

const SHOP_TABS: [&str; 2] = ["Tug", "Parts"];

pub fn shop(
    look: &Look,
    u: &mut Ui,
    input: &MenuInput,
    view: (f32, f32),
    dpi: f32,
    v: ShopView,
    rng: &mut Rng,
) -> Option<Act> {
    let (w, h) = (1280.0, 760.0);
    u.begin(input, (w, h), view, dpi);
    if input.tab != 0 {
        *v.tab = (*v.tab as i32 + input.tab).rem_euclid(SHOP_TABS.len() as i32) as usize;
    }
    let r = Rect::new(20.0, 20.0, 1240.0, 720.0);
    card(r);
    let career = v.career;
    let cls = career.tug();
    let (x, y0) = (r.x + 24.0, r.y + 26.0);
    let mut act = None;
    let eyebrow = format!(
        "Before shift {} · {} · {} \"{}\" · Hollis-3 machine shop",
        career.shift,
        career.mode.def().label,
        cls.mk,
        cls.name
    );
    label(look, u, &eyebrow, x, y0, c(col::MUTED));
    text(
        look,
        u,
        "Outfitting",
        x,
        y0 + 20.0,
        64.0,
        Which::Display,
        c(col::HAZARD),
    );
    // (Marge's shop commentary arrives with her radio in milestone 5.)
    let wallet = if career.wallet < 0.0 {
        format!(
            "{} (limit {})",
            neg(-career.wallet),
            cr(career.overdraft_limit())
        )
    } else {
        cr(career.wallet)
    };
    let rows = [
        (
            "Wallet",
            wallet,
            if career.wallet < 0.0 {
                col::DANGER
            } else {
                col::HAZARD
            },
        ),
        ("Loan balance", cr(career.loan), col::TEXT),
        ("Due after next shift", cr(career.due(0)), col::TEXT),
    ];
    let lw: f32 = {
        let a = rows
            .iter()
            .map(|r| text_w(look, r.0, 13.0, Which::MonoLight))
            .fold(0.0, f32::max);
        let b = rows
            .iter()
            .map(|r| text_w(look, &r.1, 13.0, Which::MonoLight))
            .fold(0.0, f32::max);
        a + 18.0 + b
    };
    ui::ledger(look, u, r.x + r.w - 24.0 - lw, y0 + 14.0, &rows);

    // Tabs. (Jobs & crew and Map & standing arrive in milestone 5.)
    let ty = y0 + 96.0;
    let mut tx = x;
    for (i, name) in SHOP_TABS.iter().enumerate() {
        let tr = ui::button_rect(look, tx, ty, Style::Tab { selected: false }, name);
        if u.button(
            look,
            &format!("tab-{i}"),
            tr,
            Style::Tab {
                selected: *v.tab == i,
            },
            name,
            true,
        ) {
            *v.tab = i;
        }
        tx += tr.w;
    }
    let body_y = ty + 33.0;
    draw_line(x, body_y, r.x + r.w - 24.0, body_y, 1.0, c(col::PANEL_EDGE));
    let body = Rect::new(x, body_y + 12.0, r.w - 48.0, 470.0);
    let picked = if *v.tab == 0 {
        tug_tab(look, u, career, body, v.t, rng)
    } else {
        parts_tab(look, u, career, body)
    };
    act = act.or(picked);

    // Actions.
    let ay = r.y + r.h - 70.0;
    let start = format!("Start shift {}", career.shift);
    let (p, sr) = u.button_at(look, "next", x, ay, Style::Primary, &start, true);
    if p {
        act = Some(Act::StartShift);
    }
    let by = ay + (sr.h - 37.0) / 2.0;
    let pay = 1000f64.min(career.wallet).min(career.loan);
    let lbl = if pay > 0.0 {
        format!("Pay {} extra on the loan", cr(pay))
    } else {
        "Nothing to pay with".to_owned()
    };
    let (p, pr) = u.button_at(
        look,
        "pay",
        sr.x + sr.w + 12.0,
        by,
        Style::Ghost,
        &lbl,
        pay > 0.0,
    );
    if p {
        act = Some(Act::Pay(1000.0));
    }
    let mut bx = pr.x + pr.w + 12.0;
    // "Pay everything" only matters when it's more than the 1,000 step.
    let all = career.wallet.min(career.loan);
    if all > 1000.0 {
        let lbl = if all >= career.loan {
            format!("Pay off the tug ({})", cr(all))
        } else {
            format!("Pay all {}", cr(all))
        };
        let (p, ar) = u.button_at(look, "payall", bx, by, Style::Ghost, &lbl, true);
        if p {
            act = Some(Act::Pay(f64::INFINITY));
        }
        bx = ar.x + ar.w + 12.0;
    }
    let (p, mr) = u.button_at(look, "menu", bx, by, Style::Ghost, "Main menu", true);
    if p {
        act = Some(Act::ToMenu);
    }
    text(
        look,
        u,
        v.msg,
        mr.x + mr.w + 16.0,
        by + 10.0,
        12.0,
        Which::MonoLight,
        c(col::CABLE),
    );
    act
}

/// The Tug tab: preview bench and hardpoints, tug systems, weapon systems.
fn tug_tab(
    look: &Look,
    u: &mut Ui,
    career: &Career,
    r: Rect,
    t: f32,
    rng: &mut Rng,
) -> Option<Act> {
    let mut act = None;
    // The bench: a floor grid with the tug turning slowly on it.
    let bw = 380.0;
    let pv = Rect::new(r.x, r.y, bw, 190.0);
    draw_rectangle(pv.x, pv.y, pv.w, pv.h, c(0x070a12));
    let grid = rgba(col::PANEL_EDGE, 0.6);
    let mut gx = pv.x;
    while gx < pv.x + pv.w {
        draw_line(gx, pv.y, gx, pv.y + pv.h, 1.0, grid);
        gx += 24.0;
    }
    let mut gy = pv.y;
    while gy < pv.y + pv.h {
        draw_line(pv.x, gy, pv.x + pv.w, gy, 1.0, grid);
        gy += 24.0;
    }
    // The fully kitted tug spans ~70 units with barrels and A-frame.
    let zoom = pv.w.min(pv.h * 1.5) / 130.0;
    let f = Frame::new(
        pv.x + pv.w / 2.0,
        pv.y + pv.h / 2.0,
        -0.35 + (t * 0.6).sin() * 0.08,
        zoom,
    );
    let st = TugState {
        t,
        sentries: career.sentries,
        ..TugState::default()
    };
    tug::draw_tug(look, &f, &career.mods(), &st, rng);

    // Hardpoints: two columns, the cannon mod spanning both.
    let mut sy = pv.y + pv.h + 10.0;
    let half = (bw - 8.0) / 2.0;
    let mut col_i = 0;
    for (i, s) in SLOTS.iter().enumerate() {
        let part = career.mounted[i];
        let wide = s.kind == SlotType::Mod;
        if wide && col_i == 1 {
            col_i = 0;
            sy += 52.0;
        }
        let sx = r.x
            + if wide {
                0.0
            } else {
                col_i as f32 * (half + 8.0)
            };
        let sr = Rect::new(sx, sy, if wide { bw } else { half }, 44.0);
        let name = part.map_or("Empty", |p| p.def().name);
        if u.button(
            look,
            &format!("slot-{}", s.key),
            sr,
            Style::Slot {
                filled: part.is_some(),
            },
            "",
            part.is_some(),
        ) {
            act = part.map(Act::Unmount);
        }
        label(look, u, s.label, sr.x + 10.0, sr.y + 7.0, c(col::MUTED));
        let fg = if part.is_some() {
            col::TEXT
        } else {
            col::MUTED
        };
        text(
            look,
            u,
            name,
            sr.x + 10.0,
            sr.y + 21.0,
            15.0,
            Which::Body,
            c(fg),
        );
        if wide {
            sy += 52.0;
        } else {
            col_i = (col_i + 1) % 2;
            if col_i == 0 {
                sy += 52.0;
            }
        }
    }
    let note = "Select a filled hardpoint to unmount it. Parts you own stay in the shop for free.";
    paragraph(
        look,
        u,
        note,
        r.x,
        sy + 4.0,
        bw,
        12.0,
        Which::MonoLight,
        c(col::MUTED),
        1.4,
    );

    // Systems columns.
    let cw = (r.w - bw - 48.0) / 2.0;
    let x1 = r.x + bw + 24.0;
    let x2 = x1 + cw + 24.0;
    let mut y1 = r.y;
    let mut y2 = r.y;
    section(look, u, "Tug systems", x1, &mut y1);
    section(look, u, "Weapon systems", x2, &mut y2);
    for s in SYSTEMS {
        let weapons = s.needs().is_some();
        let (sx, sy) = if weapons {
            (x2, &mut y2)
        } else {
            (x1, &mut y1)
        };
        if let Some(a) = system_row(look, u, career, s, sx, sy, cw) {
            act = Some(a);
        }
    }
    sentry_row(look, u, career, x2, &mut y2, cw);
    act
}

fn section(look: &Look, u: &Ui, name: &str, x: f32, y: &mut f32) {
    ui::text_spaced(
        look,
        u,
        &name.to_uppercase(),
        x,
        *y,
        11.0,
        Which::Mono,
        c(col::CABLE),
        11.0 * 0.14,
    );
    *y += 20.0;
}

/// One levelled system (.sys): name and pips, now and next, and the price.
fn system_row(
    look: &Look,
    u: &mut Ui,
    career: &Career,
    s: System,
    x: f32,
    y: &mut f32,
    w: f32,
) -> Option<Act> {
    let lv = career.levels.get(s);
    let max = s.max();
    let locked = s.needs().is_some_and(|p| !career.owns(p));
    let tw = w - 24.0 - 120.0;
    let now = format!("Now: {}", s.describe(lv));
    let next = (lv < max).then(|| format!("Next: {}", s.describe(lv + 1)));
    let h = 12.0
        + 26.0
        + paragraph_h(look, &now, tw, 14.0, Which::Body, 1.3)
        + next
            .as_ref()
            .map_or(0.0, |n| paragraph_h(look, n, tw, 14.0, Which::Body, 1.3))
        + 10.0;
    let h = h.max(70.0);
    let r = Rect::new(x, *y, w, h);
    draw_rectangle(r.x, r.y, r.w, r.h, c(0x0b101c));
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, c(col::PANEL_EDGE));
    let (ix, mut iy) = (r.x + 12.0, r.y + 8.0);
    let nw = text(
        look,
        u,
        s.name(),
        ix,
        iy,
        20.0,
        Which::Display,
        c(col::TEXT),
    );
    ui::pips(
        ix + nw + 8.0,
        iy + 13.0,
        usize::from(lv),
        usize::from(max),
        13.0,
        c(col::HAZARD),
    );
    iy += 28.0;
    iy += paragraph(
        look,
        u,
        &now,
        ix,
        iy,
        tw,
        14.0,
        Which::Body,
        c(col::MUTED),
        1.3,
    );
    if let Some(n) = &next {
        paragraph(look, u, n, ix, iy, tw, 14.0, Which::Body, c(col::TEXT), 1.3);
    }
    let mut act = None;
    let right = r.x + r.w - 12.0;
    if locked {
        let need = format!("Needs {}", s.needs().expect("weapon system").def().name);
        status(look, u, &need, right, r.y + 12.0, col::MUTED);
    } else if lv >= max {
        status(look, u, "Maxed", right, r.y + 12.0, col::CABLE);
    } else {
        let price = career.level_price(s).expect("buyable");
        status(look, u, &cr(price), right, r.y + 12.0, col::HAZARD);
        let lbl = format!("Level {}", lv + 1);
        let br = ui::button_rect(look, 0.0, 0.0, Style::Buy, &lbl);
        let br = Rect::new(right - br.w, r.y + 34.0, br.w, br.h);
        if u.button(
            look,
            &format!("sys-{}", s.key()),
            br,
            Style::Buy,
            &lbl,
            career.wallet >= price,
        ) {
            act = Some(Act::Level(s));
        }
    }
    *y += h + 6.0;
    act
}

/// Right-aligned status text (.status) ending at `right`.
fn status(look: &Look, u: &Ui, s: &str, right: f32, y: f32, color: u32) {
    let w = text_w(look, s, 12.0, Which::MonoLight);
    text(look, u, s, right - w, y, 12.0, Which::MonoLight, c(color));
}

/// The sentry drone row. Sentries fly with the drone bay in milestone 4;
/// until then the row explains what they need and nothing is sold.
fn sentry_row(look: &Look, u: &mut Ui, career: &Career, x: f32, y: &mut f32, w: f32) {
    let s = "Needs a drone bay (utility part). Drone bays and sentries arrive with milestone 4 of the port.";
    let h = 44.0 + paragraph_h(look, s, w - 140.0, 14.0, Which::Body, 1.3);
    let r = Rect::new(x, *y, w, h);
    draw_rectangle(r.x, r.y, r.w, r.h, c(0x0b101c));
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, c(col::PANEL_EDGE));
    let nw = text(
        look,
        u,
        "Sentry drones",
        r.x + 12.0,
        r.y + 8.0,
        20.0,
        Which::Display,
        c(col::TEXT),
    );
    let n = career.sentries as usize;
    let pips = "◆".repeat(n) + &"◇".repeat(SENTRY_MAX as usize - n);
    ui::text_spaced(
        look,
        u,
        &pips,
        r.x + 12.0 + nw + 8.0,
        r.y + 13.0,
        13.0,
        Which::Mono,
        c(col::HAZARD),
        13.0 * 0.15,
    );
    paragraph(
        look,
        u,
        s,
        r.x + 12.0,
        r.y + 36.0,
        w - 140.0,
        14.0,
        Which::Body,
        c(col::MUTED),
        1.3,
    );
    status(
        look,
        u,
        &cr(SENTRY_PRICE),
        r.x + r.w - 12.0,
        r.y + 12.0,
        col::HAZARD,
    );
    *y += r.h + 6.0;
}

/// The Parts tab: a grid of compact cards (#shop-catalog), each with what
/// the part does, what it costs you, and buy / mount / unmount.
fn parts_tab(look: &Look, u: &mut Ui, career: &Career, r: Rect) -> Option<Act> {
    let mut act = None;
    let mut y = r.y;
    section(look, u, "Parts", r.x, &mut y);
    // repeat(auto-fill, minmax(225px, 1fr)) with an 8 px gap.
    let gap = 8.0;
    let cols = ((r.w + gap) / (225.0 + gap)).floor().max(1.0) as usize;
    let cw = (r.w - gap * (cols as f32 - 1.0)) / cols as f32;
    let ch = 140.0;
    for (i, p) in PARTS.into_iter().enumerate() {
        let cx = r.x + (i % cols) as f32 * (cw + gap);
        let cy = y + (i / cols) as f32 * (ch + gap);
        if let Some(a) = part_card(look, u, career, p, Rect::new(cx, cy, cw, ch)) {
            act = Some(a);
        }
    }
    act
}

/// At most `n` wrapped lines, the last cut with an ellipsis if text was
/// dropped (-webkit-line-clamp).
fn clamp_lines(look: &Look, s: &str, size: f32, w: f32, n: usize) -> Vec<String> {
    let mut lines = ui::wrap(look, s, size, Which::Body, w);
    if lines.len() > n {
        lines.truncate(n);
        let last = lines.last_mut().expect("n > 0");
        while !last.is_empty() && text_w(look, &format!("{last}…"), size, Which::Body) > w {
            last.pop();
        }
        last.push('…');
    }
    lines
}

fn part_card(look: &Look, u: &mut Ui, career: &Career, p: Part, r: Rect) -> Option<Act> {
    let d = p.def();
    let owned = career.owns(p);
    let mounted = career.is_mounted(p);
    draw_rectangle(r.x, r.y, r.w, r.h, c(0x0b101c));
    let edge = if mounted { col::CABLE } else { col::PANEL_EDGE };
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, c(edge));
    let (ix, iw) = (r.x + 10.0, r.w - 20.0);
    let mut iy = r.y + 8.0;
    // The name, with its hardpoint tag beside it, or below if it won't fit.
    let nw = text(look, u, d.name, ix, iy, 22.0, Which::Display, c(col::TEXT));
    let tag = if d.slot == SlotType::Mod {
        "cannon mod"
    } else {
        d.slot.label()
    }
    .to_uppercase();
    let tag_w = text_w(look, &tag, 10.0, Which::MonoLight) + tag.len() as f32 * 1.4;
    if nw + 8.0 + tag_w <= iw {
        ui::text_spaced(
            look,
            u,
            &tag,
            ix + nw + 8.0,
            iy + 9.0,
            10.0,
            Which::MonoLight,
            c(col::MUTED),
            1.4,
        );
        iy += 28.0;
    } else {
        ui::text_spaced(
            look,
            u,
            &tag,
            ix,
            iy + 26.0,
            10.0,
            Which::MonoLight,
            c(col::MUTED),
            1.4,
        );
        iy += 40.0;
    }
    let lh = 12.5 * 1.35;
    for l in clamp_lines(look, d.does, 12.5, iw, 2) {
        text(look, u, &l, ix, iy, 12.5, Which::Body, c(col::TEXT));
        iy += lh;
    }
    for l in clamp_lines(look, d.cost, 12.5, iw, 1) {
        text(look, u, &l, ix, iy, 12.5, Which::Body, c(col::TRADEOFF));
    }

    let id = format!("part-{}", d.key);
    let (state, state_c, btn, style, enabled, act) = if !owned && d.unique {
        (
            "Chain reward".to_owned(),
            col::MUTED,
            "Locked",
            Style::Act,
            false,
            None,
        )
    } else if !owned && p.pending().is_some() {
        (
            format!("In {}", p.pending().unwrap_or_default()),
            col::MUTED,
            "Not yet",
            Style::Act,
            false,
            None,
        )
    } else if !owned {
        let can = career.wallet >= d.price;
        (
            cr(d.price),
            col::HAZARD,
            "Buy",
            Style::Buy,
            can,
            Some(Act::Buy(p)),
        )
    } else if mounted {
        (
            "Mounted".to_owned(),
            col::CABLE,
            "Unmount",
            Style::Act,
            true,
            Some(Act::Unmount(p)),
        )
    } else {
        let free = career.free_slot(d.slot).is_some() && p.pending().is_none();
        let s = if free {
            "Owned".to_owned()
        } else {
            format!("{} full", d.slot.label())
        };
        (
            s,
            col::MUTED,
            "Mount",
            Style::Act,
            free,
            Some(Act::Mount(p)),
        )
    };
    // The buy row along the bottom: status on the left, the button right.
    let br = ui::button_rect(look, 0.0, 0.0, style, btn);
    let br = Rect::new(r.x + r.w - 10.0 - br.w, r.y + r.h - 8.0 - br.h, br.w, br.h);
    text(
        look,
        u,
        &state,
        ix,
        br.y + (br.h - 12.0 * 1.2) / 2.0,
        12.0,
        Which::MonoLight,
        c(state_c),
    );
    if u.button(look, &id, br, style, btn, enabled) {
        return act;
    }
    None
}
