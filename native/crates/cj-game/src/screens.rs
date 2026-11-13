//! The menu screens: title, pause, shift report and machine shop (the
//! #screen-* cards in src/page.html, filled by src/js/18-hud-and-screens.js).
//! Each draws itself through the `ui` kit and returns what the player
//! chose; main.rs carries the choice out.

use crate::look::{col, rgba, Look};
use crate::tug::{self, Frame, TugState};
use crate::ui::{
    self, card, label, paragraph, paragraph_h, text, text_w, MenuInput, Style, Ui, Which,
};
use cj_sim::career::{rank_for, tier, Career, Mode, Outcome, Settlement, Tier, MODES, REP_NAMES};
use cj_sim::field::{truce_stage, Belt, Condition, Crew, BELTS, CORE_SELL, CREWS};
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
    BuySentry,
    /// Accept or drop contract offer i.
    Offer(usize),
    Crew(Crew),
    Belt(Belt),
    Truce,
    SellCore,
    ReprogramCore,
    OpenSettings,
    OpenRecords,
    OpenBoard,
    OpenSounds,
    Daily,
    ReplayTraining,
    /// Start or finish editing the pilot name.
    EditName,
    ToggleVoice,
    SetMusic,
    SetMotion,
    SetPalette,
    SetEffects,
    SetSprites,
    CycleHud,
    /// Step one Sound row's volume (index into `Settings::volume`).
    CycleVolume(usize),
    CycleCoop,
    Rebind(crate::settings::Bind),
    ResetKeys,
    /// Leave a menu screen for the one it was opened from.
    Back,
    /// The launch choice: play alone, host a server, or join one.
    SinglePlayer,
    Host,
    /// Step the hosted server's player limit (2 to 20).
    CycleLimit,
    /// Step the hosted server's career difficulty mode.
    CycleHostMode,
    /// Switch PvP on or off for a server this player hosts.
    ToggleHostPvp,
    OpenJoin,
    /// Look up the server at the address typed on the Join screen, or,
    /// once it has answered, join it.
    Join,
    /// In the break between online shifts: ready for the next one.
    Ready,
    /// Leave the server from the lobby.
    LeaveServer,
    /// Start typing a line for the public chat.
    OpenChat,
    /// Open the shop from the lobby in the break between online shifts.
    OpenShop,
    BoardTab(usize),
    PlaySound(usize),
    PlayAll,
    StopSounds,
    Paint(usize),
}

fn c(rgb: u32) -> Color {
    crate::look::hex(rgb)
}

fn cr(n: f64) -> String {
    format!("{} cr", fmt_credits(n))
}

/// Negative credits with a true minus sign, as the browser writes them.
fn neg(n: f64) -> String {
    format!("\u{2212}{}", cr(n))
}

/* ------------------------------- Title -------------------------------- */

/// What the title screen shows, borrowed from the app's state.
pub struct TitleView<'a> {
    /// The pilot name, and the text being typed while it is edited.
    pub pilot: &'a str,
    pub editing: Option<&'a str>,
    pub name_msg: &'a str,
    pub saved: Option<&'a Career>,
    pub mode: Mode,
    pub best: Option<f64>,
    /// Field-guide tab (0 keyboard, 1 gamepad, 2 bay rates, 3 notes).
    pub tab: &'a mut usize,
    /// "Erase saved career? Press again" is showing.
    pub armed_new: bool,
}

const GUIDE_TABS: [&str; 4] = ["Keyboard", "Gamepad", "Bay rates", "Field notes"];

/// Draws the title screen and returns the action the player picked, if any. Tab input cycles the field-guide tabs.
pub fn title(
    look: &Look,
    u: &mut Ui,
    input: &MenuInput,
    view: (f32, f32),
    dpi: f32,
    v: TitleView,
) -> Option<Act> {
    let (w, h) = (1200.0, 660.0);
    u.begin(input, (w, h), view, dpi);
    if input.tab != 0 {
        *v.tab = (*v.tab as i32 + input.tab).rem_euclid(4) as usize;
    }
    let r = Rect::new(20.0, 20.0, 1160.0, 620.0);
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
    ) + 16.0;

    // Pilot name (shown on the leaderboard): a field edited in place.
    label(
        look,
        u,
        "Pilot name (shown on the leaderboard)",
        x,
        y,
        c(col::MUTED),
    );
    y += 18.0;
    let shown = match v.editing {
        Some(t) => format!("{t}_"),
        None => v.pilot.to_owned(),
    };
    let nr = Rect::new(x, y, 300.0, 34.0);
    if u.button(
        look,
        "pilot",
        nr,
        Style::Belt {
            picked: v.editing.is_some(),
        },
        "",
        true,
    ) {
        act = Some(Act::EditName);
    }
    text(
        look,
        u,
        &shown,
        nr.x + 10.0,
        nr.y + 8.0,
        15.0,
        Which::Body,
        c(col::TEXT),
    );
    let edit = if v.editing.is_some() {
        "Save name"
    } else {
        "Edit name"
    };
    let (p, er) = u.button_at(
        look,
        "name-save",
        nr.x + nr.w + 8.0,
        y,
        Style::Ghost,
        edit,
        true,
    );
    if p {
        act = Some(Act::EditName);
    }
    text(
        look,
        u,
        v.name_msg,
        er.x + er.w + 10.0,
        y + 11.0,
        12.0,
        Which::MonoLight,
        c(col::CABLE),
    );
    y += 46.0;

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
    y += sr.h + 10.0;
    // The other screens: the browser's row of ghost buttons.
    let mut mx = x;
    let mut my = y;
    for (id, lbl, a) in [
        ("board", "Leaderboard", Act::OpenBoard),
        ("sounds", "Sound test", Act::OpenSounds),
        ("daily", "Daily challenge", Act::Daily),
        ("training", "Replay training", Act::ReplayTraining),
        ("records", "Records", Act::OpenRecords),
        ("settings", "Settings", Act::OpenSettings),
    ] {
        let br = ui::button_rect(look, mx, my, Style::Ghost, lbl);
        if mx + br.w > x + 580.0 {
            mx = x;
            my += br.h + 8.0;
        }
        let (p, br) = u.button_at(look, id, mx, my, Style::Ghost, lbl, true);
        if p {
            act = Some(a);
        }
        mx += br.w + 8.0;
    }
    y = my + 45.0;
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
            ("Shift", "Afterburner (heats the engine)"),
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
            ("LT + RT", "Afterburner (heats the engine)"),
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

/// Draws the pause menu and returns the action the player picked, if any. `voice_on` is Marge's voice setting and `armed_menu` is whether the main-menu button is waiting for a second press to end the shift.
/// The first screen: play alone, host a server for others, or join one.
/// `limit`, `mode` and `pvp` are the options for a server this player hosts.
// The screen's inputs plus the three host options, like the other screens'.
#[allow(clippy::too_many_arguments)]
pub fn launch(
    look: &Look,
    u: &mut Ui,
    input: &MenuInput,
    view: (f32, f32),
    dpi: f32,
    limit: usize,
    mode: Mode,
    pvp: bool,
) -> Option<Act> {
    let (w, h) = (900.0, 350.0);
    u.begin(input, (w, h), view, dpi);
    let r = Rect::new(20.0, 20.0, 860.0, 310.0);
    card(r);
    copyright(look, u, r);
    let (x, mut y) = (r.x + 36.0, r.y + 34.0);
    label(look, u, "Hollis Belt Salvage", x, y, c(col::MUTED));
    y += 22.0;
    text(
        look,
        u,
        "Claim Jumpers",
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
        "Fly your own career, or share a shift with other tugs.",
        x,
        y,
        18.0,
        Which::Body,
        c(col::TEXT),
    );
    y += 44.0;
    let mut act = None;
    let (p, r1) = u.button_at(look, "single", x, y, Style::Primary, "Single player", true);
    if p {
        act = Some(Act::SinglePlayer);
    }
    let by = y + (r1.h - 37.0) / 2.0;
    let (p, r2) = u.button_at(
        look,
        "host",
        r1.x + r1.w + 12.0,
        by,
        Style::Ghost,
        "Host server",
        true,
    );
    if p {
        act = Some(Act::Host);
    }
    let (p, r3) = u.button_at(
        look,
        "join",
        r2.x + r2.w + 12.0,
        by,
        Style::Ghost,
        "Join remote host",
        true,
    );
    if p {
        act = Some(Act::OpenJoin);
    }
    if u.button_at(
        look,
        "quit",
        r3.x + r3.w + 12.0,
        by,
        Style::Ghost,
        "Quit",
        true,
    )
    .0
    {
        act = Some(Act::Quit);
    }
    // The field a hosted server makes grows with how many it's for.
    let lim = format!("Host for up to {limit} players");
    let (p, lr) = u.button_at(look, "limit", r2.x, by + 50.0, Style::Ghost, &lim, true);
    if p {
        act = Some(Act::CycleLimit);
    }
    // Every career on a hosted server plays at the host's chosen mode.
    let m = format!("Careers: {}", mode.def().label);
    let (p, mr) = u.button_at(
        look,
        "mode",
        lr.x + lr.w + 12.0,
        by + 50.0,
        Style::Ghost,
        &m,
        true,
    );
    if p {
        act = Some(Act::CycleHostMode);
    }
    // Off by default: a hosted server is co-op unless the host opts in.
    let pv = if pvp { "PvP: On" } else { "PvP: Off" };
    if u.button_at(
        look,
        "pvp",
        mr.x + mr.w + 12.0,
        by + 50.0,
        Style::Ghost,
        pv,
        true,
    )
    .0
    {
        act = Some(Act::ToggleHostPvp);
    }
    act
}

/// The copyright notice (set by build.rs), small and muted along the
/// bottom-left of a card.
fn copyright(look: &Look, u: &Ui, r: Rect) {
    text(
        look,
        u,
        env!("CJ_COPYRIGHT"),
        r.x + 36.0,
        r.y + r.h - 28.0,
        12.0,
        Which::Body,
        c(col::MUTED),
    );
}

/// Shown instead of the game once the play-test cut-off has passed (see
/// `expiry`). Returns true when the player presses Quit.
pub fn expired(look: &Look, u: &mut Ui, input: &MenuInput, view: (f32, f32), dpi: f32) -> bool {
    let (w, h) = (900.0, 330.0);
    u.begin(input, (w, h), view, dpi);
    let r = Rect::new(20.0, 20.0, 860.0, 290.0);
    card(r);
    copyright(look, u, r);
    let (x, mut y) = (r.x + 36.0, r.y + 34.0);
    label(look, u, "Play-test build", x, y, c(col::MUTED));
    y += 22.0;
    text(
        look,
        u,
        "Claim Jumpers",
        x,
        y,
        64.0,
        Which::Display,
        c(col::HAZARD),
    );
    y += 72.0;
    // The saves are never touched here, so a newer build picks them up.
    let why = format!(
        "This play-test build stopped working on {}. Ask for a newer build to keep playing. Your career and settings are kept for it.",
        crate::expiry::CUT_OFF_TEXT
    );
    y += paragraph(
        look,
        u,
        &why,
        x,
        y,
        760.0,
        18.0,
        Which::Body,
        c(col::TEXT),
        1.45,
    ) + 20.0;
    u.button_at(look, "quit", x, y, Style::Primary, "Quit", true)
        .0
}

/// What the Join screen shows besides the address being typed.
pub struct JoinView<'a> {
    pub address: &'a str,
    /// What the server said about itself (or that it's being asked), and
    /// the colour to say it in.
    pub status: Option<(String, u32)>,
    /// The server has answered and has room: the button joins.
    pub can_join: bool,
    /// The server is being looked up: the button waits.
    pub looking: bool,
}

/// Joining a server: the host's address is typed in (the port is always
/// 47820), then the server is looked up before joining.
pub fn join(
    look: &Look,
    u: &mut Ui,
    input: &MenuInput,
    view: (f32, f32),
    dpi: f32,
    v: JoinView,
) -> Option<Act> {
    let address = v.address;
    let (w, h) = (900.0, 360.0);
    u.begin(input, (w, h), view, dpi);
    let r = Rect::new(20.0, 20.0, 860.0, 320.0);
    card(r);
    let (x, mut y) = (r.x + 36.0, r.y + 34.0);
    label(look, u, "Join remote host", x, y, c(col::MUTED));
    y += 22.0;
    text(
        look,
        u,
        "Whose shift?",
        x,
        y,
        48.0,
        Which::Display,
        c(col::HAZARD),
    );
    y += 62.0;
    label(
        look,
        u,
        "The host's IP address (port 47820)",
        x,
        y,
        c(col::MUTED),
    );
    y += 18.0;
    let field = Rect::new(x, y, 360.0, 34.0);
    u.button(
        look,
        "address",
        field,
        Style::Belt { picked: true },
        "",
        true,
    );
    text(
        look,
        u,
        &format!("{address}_"),
        field.x + 10.0,
        field.y + 8.0,
        15.0,
        Which::Body,
        c(col::TEXT),
    );
    y += 44.0;
    if let Some((s, rgb)) = &v.status {
        text(look, u, s, x, y, 15.0, Which::Body, c(*rgb));
    }
    y += 30.0;
    let mut act = None;
    let (go, ok) = if v.looking {
        ("Join", v.can_join)
    } else {
        ("Find server", !address.is_empty())
    };
    let (p, jr) = u.button_at(look, "go", x, y, Style::Primary, go, ok);
    if p {
        act = Some(Act::Join);
    }
    let by = y + (jr.h - 37.0) / 2.0;
    if u.button_at(
        look,
        "back",
        jr.x + jr.w + 12.0,
        by,
        Style::Ghost,
        "Back",
        true,
    )
    .0 || input.back
    {
        act = Some(Act::Back);
    }
    // Every server is sent the same identity (mp_id.txt), so a host could
    // reuse it elsewhere: say so where the choice to join is made.
    text(
        look,
        u,
        "Your game sends the host your pilot's identity. Join only hosts you trust.",
        x,
        y + jr.h + 18.0,
        15.0,
        Which::Body,
        c(col::MUTED),
    );
    act
}

/// What the lobby screen shows: the server's list, and where this
/// player stands.
pub struct LobbyView<'a> {
    pub lobby: Option<&'a cj_net::Lobby>,
    /// Waiting to be let into the next shift (rather than between shifts).
    pub waiting: bool,
    /// This player has said they are ready.
    pub ready: bool,
    /// The server has given this player their career, so the shop can open.
    pub can_shop: bool,
}

/// The lobby between online shifts, and for players waiting to join one:
/// who is on the server, this shift's scores, and the countdown.
pub fn lobby(
    look: &Look,
    u: &mut Ui,
    input: &MenuInput,
    view: (f32, f32),
    dpi: f32,
    v: LobbyView,
) -> Option<Act> {
    let rows = v.lobby.map_or(0, |l| l.players.len());
    let h = 250.0 + 24.0 * rows as f32;
    u.begin(input, (900.0, h + 40.0), view, dpi);
    let r = Rect::new(20.0, 20.0, 860.0, h);
    card(r);
    let (x, mut y) = (r.x + 36.0, r.y + 34.0);
    let (kicker, title) = match (v.waiting, v.lobby.map(|l| l.phase)) {
        (true, Some(cj_net::Phase::Shift { .. })) => ("In the lobby", "Next shift soon"),
        (true, _) => ("In the lobby", "Between shifts"),
        (false, _) => ("Shift over", "Between shifts"),
    };
    label(look, u, kicker, x, y, c(col::MUTED));
    y += 22.0;
    text(look, u, title, x, y, 48.0, Which::Display, c(col::HAZARD));
    y += 62.0;
    let line = match v.lobby.map(|l| l.phase) {
        Some(cj_net::Phase::Shift { time_left }) => format!(
            "This shift ends in {}:{:02}. You fly in the next one.",
            (time_left / 60.0) as u32,
            (time_left % 60.0) as u32
        ),
        Some(cj_net::Phase::Break { seconds_left }) => format!(
            "The next shift starts when everyone is ready, or in {} s.",
            seconds_left.ceil() as u32
        ),
        None => "Asking the server who is here...".to_owned(),
    };
    text(look, u, &line, x, y, 15.0, Which::Body, c(col::TEXT));
    y += 24.0;
    // The belt votes so far, in the break.
    if let Some(l) = v.lobby.filter(|l| !l.votes.is_empty()) {
        let votes: Vec<String> = l
            .votes
            .iter()
            .map(|(b, n)| format!("{} {n}", b.def().name))
            .collect();
        let s = format!("Belt votes: {}", votes.join(" · "));
        text(look, u, &s, x, y, 13.0, Which::Body, c(col::MUTED));
    }
    y += 22.0;
    if let Some(l) = v.lobby {
        let cols = [x, x + 330.0, x + 470.0, x + 560.0, x + 650.0];
        for (i, head) in ["Pilot", "Banked", "Kills", "Deaths", ""]
            .iter()
            .enumerate()
        {
            label(look, u, head, cols[i], y, c(col::MUTED));
        }
        y += 20.0;
        for p in &l.players {
            let state = match (p.slot, p.away, p.ready) {
                (None, _, true) | (Some(_), false, true) => "ready",
                (None, _, false) => "waiting",
                (Some(_), true, _) => "away",
                (Some(_), false, false) => "",
            };
            let cells = [
                p.name.clone(),
                fmt_credits(p.banked as f64),
                p.kills.to_string(),
                p.deaths.to_string(),
                state.to_owned(),
            ];
            for (i, s) in cells.iter().enumerate() {
                text(look, u, s, cols[i], y, 15.0, Which::Body, c(col::TEXT));
            }
            y += 24.0;
        }
    }
    y += 16.0;
    let mut act = None;
    let in_break = matches!(v.lobby.map(|l| l.phase), Some(cj_net::Phase::Break { .. }));
    let ready_label = if v.ready { "Ready" } else { "I'm ready" };
    let (p, rr) = u.button_at(
        look,
        "ready",
        x,
        y,
        Style::Primary,
        ready_label,
        in_break && !v.ready,
    );
    if p {
        act = Some(Act::Ready);
    }
    let by = y + (rr.h - 37.0) / 2.0;
    let (p, sr) = u.button_at(
        look,
        "shop",
        rr.x + rr.w + 12.0,
        by,
        Style::Ghost,
        "Shop & belt",
        in_break && v.can_shop,
    );
    if p {
        act = Some(Act::OpenShop);
    }
    let (p, cr) = u.button_at(
        look,
        "chat",
        sr.x + sr.w + 12.0,
        by,
        Style::Ghost,
        "Chat",
        true,
    );
    if p {
        act = Some(Act::OpenChat);
    }
    if u.button_at(
        look,
        "leave",
        cr.x + cr.w + 12.0,
        by,
        Style::Ghost,
        "Leave server",
        true,
    )
    .0
    {
        act = Some(Act::LeaveServer);
    }
    act
}

pub fn pause(
    look: &Look,
    u: &mut Ui,
    input: &MenuInput,
    view: (f32, f32),
    dpi: f32,
    armed_menu: bool,
    voice_on: bool,
) -> Option<Act> {
    let (w, h) = (900.0, 360.0);
    u.begin(input, (w, h), view, dpi);
    let r = Rect::new(20.0, 20.0, 860.0, 320.0);
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
    let vl = format!("Marge's voice: {}", if voice_on { "on" } else { "off" });
    let (p, vr) = u.button_at(
        look,
        "voice",
        rr.x + rr.w + 12.0,
        by,
        Style::Ghost,
        &vl,
        true,
    );
    if p {
        act = Some(Act::ToggleVoice);
    }
    let (p, _) = u.button_at(
        look,
        "settings",
        vr.x + vr.w + 12.0,
        by,
        Style::Ghost,
        "Settings",
        true,
    );
    if p {
        act = Some(Act::OpenSettings);
    }
    let by = by + 50.0;
    let (p, er) = u.button_at(
        look,
        "end",
        r.x + 36.0,
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
    /// Kills by kind (KILL_KINDS): count, plural and singular; the
    /// first four are rival crews.
    pub kills: [(u32, &'static str, &'static str); 11],
    /// The money side of the opposition (impound, confiscation, arrests).
    pub impounded: bool,
    pub impound_fee: f64,
    /// Seconds left on the clock when it ended.
    pub time_left: f64,
    pub confiscated: f64,
    pub loan_credit: f64,
    pub arrests: u32,
    pub fines: f64,
    pub fine_from_cargo: f64,
    /// The career after settling (for the wallet, loan and notes).
    pub career: Career,
    pub mode: Mode,
    pub best: f64,
    pub is_best: bool,
    /// The day of a daily challenge (its report reads differently).
    pub daily: Option<String>,
    /// A payoff from the shop: no shift behind it, so its rows read "—".
    pub no_shift: bool,
    /// The shift's highlight clip, and when the report opened (it loops).
    pub highlight: Option<crate::replay::Highlight>,
    pub shown_at: f64,
}

impl Report {
    /// Captures the report from the settlement, the world's stats and kills, the career and the best score. `is_best` marks a new best.
    pub fn new(st: Settlement, w: &World, career: &Career, best: f64, is_best: bool) -> Report {
        let k = &w.me().kills;
        Report {
            settlement: st,
            world_items: w.me().stats.items,
            best_run: w.me().stats.best_run,
            jumpers: w.me().stats.jumpers,
            lost: w.me().stats.lost,
            deaths: w.me().stats.deaths,
            bounties: w.me().bounties,
            kills: [
                (k.skiff, "skiffs", "skiff"),
                (k.cutter, "cutters", "cutter"),
                (k.brute, "brutes", "brute"),
                (k.gunship, "gunships", "gunship"),
                (k.boss, "boss crews", "boss crew"),
                (k.collector, "debt collectors", "debt collector"),
                (k.law, "law units", "law unit"),
                (k.miner, "mining drones", "mining drone"),
                (k.drone, "refinery drones", "refinery drone"),
                (k.rogue, "rogue sentries", "rogue sentry"),
                (k.jaws, "kessler jaws", "set of Kessler jaws"),
            ],
            impounded: w.me().impounded,
            impound_fee: w.me().impound_fee,
            time_left: w.time_left,
            confiscated: w.me().confiscated,
            loan_credit: w.me().loan_credit,
            arrests: w.me().arrests,
            fines: w.me().fines,
            fine_from_cargo: w.me().fine_from_cargo,
            career: career.clone(),
            mode: career.mode,
            best,
            is_best,
            daily: None,
            no_shift: false,
            highlight: None,
            shown_at: get_time(),
        }
    }

    /// Title, rank, note and main-button label (endShift and showPaidOff).
    fn words(&self) -> (String, String, String, &'static str) {
        let c = &self.career;
        let earned = self.settlement.earned;
        let rank = rank_for(earned).to_owned();
        if let Some(day) = &self.daily {
            let new = if self.is_best {
                "New best for today. "
            } else {
                ""
            };
            let note = format!(
                "{new}Everyone flew the same field today. See where you rank under Leaderboard, Daily. Your career is exactly where you left it."
            );
            return (
                format!("Daily challenge · {day}"),
                rank,
                note,
                "Back to title",
            );
        }
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
                let seized = if self.confiscated > 0.0 {
                    format!(
                        " Kessler Mutual confiscated {} of cargo and credited {} to your loan.",
                        cr(self.confiscated),
                        cr(self.loan_credit)
                    )
                } else {
                    String::new()
                };
                let law = if self.arrests > 0 {
                    let cargo = if self.fine_from_cargo > 0.0 {
                        format!(
                            ", {} of it paid with seized cargo",
                            cr(self.fine_from_cargo)
                        )
                    } else {
                        String::new()
                    };
                    format!(
                        " Arrested {}×: {} in fines{cargo}.",
                        self.arrests,
                        cr(self.fines)
                    )
                } else {
                    String::new()
                };
                let chain = &self.settlement.chain_note;
                let next = format!(
                    "Next installment is {}, due after shift {}.{overdraft}{seized}{law}{heat}{chain}",
                    cr(c.due(0)),
                    c.shift
                );
                if self.impounded {
                    let left = self.time_left.ceil().max(0.0) as i64;
                    let note = format!(
                        "Kessler Mutual towed the tug off the field with {}:{:02} still on the clock. Impound fee: {}. {next}",
                        left / 60,
                        left % 60,
                        cr(self.impound_fee)
                    );
                    return ("Impounded".to_owned(), rank, note, "Outfit the tug");
                }
                let title = if self.is_best && earned > 0.0 {
                    "New personal best".to_owned()
                } else {
                    format!("Shift {shift} complete")
                };
                let note = format!("{next} Marge suggests not spending all of it.");
                (title, rank, note, "Outfit the tug")
            }
            Outcome::Repossessed { over } => {
                let shift = c.shift;
                let impound = if self.impounded {
                    format!(
                        "Kessler Mutual impounded the tug (fee {}), and then you",
                        cr(self.impound_fee)
                    )
                } else {
                    "You".to_owned()
                };
                let note = format!(
                    "{impound} ended up {} overdrawn, {} past the bank's limit. They took the \"{}\" after {} shift{}. Marge kept your mug.",
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

/// Draws the shift report and returns the action the player picked, if any.
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
    ]
    .map(|(l, v)| (l, if rep.no_shift { "—".to_owned() } else { v }));
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
    let rivals: u32 = rep.kills[..4].iter().map(|k| k.0).sum();
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
        match &st.contract {
            None => ("Contract", "None".to_owned(), col::TEXT),
            Some((k, true)) => (
                "Contract",
                format!("+{} · {}", cr(k.reward), k.text()),
                col::TEXT,
            ),
            Some((k, false)) => ("Contract", format!("Missed · {}", k.text()), col::DANGER),
        },
        ("Standing", standing, col::TEXT),
        match c.crew.filter(|_| st.wage > 0.0) {
            Some(m) => (
                "Crew wages",
                format!("{} · {}", neg(st.wage), m.def().name),
                col::DANGER,
            ),
            None => ("Crew wages", "None".to_owned(), col::TEXT),
        },
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
    // A daily run touches no career: the career rows read "Not used".
    let rows: Vec<(&str, String, u32)> = if rep.daily.is_some() {
        rows.into_iter()
            .map(|(l, v, colr)| match l {
                "Contract" | "Standing" | "Crew wages" | "Loan installment" | "Wallet"
                | "Loan remaining" => (l, "Not used".to_owned(), col::TEXT),
                _ => (l, v, colr),
            })
            .collect()
    } else if rep.no_shift {
        rows.into_iter()
            .map(|(l, v, colr)| match l {
                "Wallet" | "Loan remaining" => (l, v, colr),
                _ => (l, "—".to_owned(), col::TEXT),
            })
            .collect()
    } else {
        rows
    };
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
    ) + 16.0;
    // The shift highlight (#end-replay): a 420 x 236 clip, scaled to the column.
    if let Some(h) = &rep.highlight {
        let rh = sw * 236.0 / 420.0;
        crate::replay::draw(h, Rect::new(sx, sy, sw, rh), get_time() - rep.shown_at);
        let cap = format!("Shift highlight · {}", h.text);
        text(
            look,
            u,
            &cap,
            sx,
            sy + rh + 4.0,
            12.0,
            Which::Body,
            self::c(col::MUTED),
        );
        sy += rh + 26.0;
    }
    sy += 8.0;
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
    let best = if rep.no_shift {
        String::new()
    } else if rep.daily.is_some() {
        format!("Your best today: {}", cr(rep.best))
    } else {
        format!(
            "Best single shift ({}): {}",
            rep.mode.def().label,
            cr(rep.best)
        )
    };
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

/// What the machine shop shows, borrowed from the app's state.
pub struct ShopView<'a> {
    pub career: &'a Career,
    pub profile: &'a crate::profile::Profile,
    /// 0 = Tug, 1 = Parts, 2 = Jobs & crew, 3 = Map & standing.
    pub tab: &'a mut usize,
    pub msg: &'a str,
    pub t: f32,
}

const SHOP_TABS: [&str; 4] = ["Tug", "Parts", "Jobs & crew", "Map & standing"];

/// Draws the machine shop and returns the action the player picked, if any. Tab input cycles its four tabs.
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
    marge_box(
        look,
        u,
        career,
        Rect::new(x + 330.0, y0 + 20.0, 560.0, 64.0),
    );
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

    // Tabs.
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
    let picked = match *v.tab {
        0 => tug_tab(look, u, career, v.profile, body, v.t, rng),
        1 => parts_tab(look, u, career, body),
        2 => jobs_tab(look, u, career, body),
        _ => map_tab(look, u, career, body),
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
    profile: &crate::profile::Profile,
    r: Rect,
    t: f32,
    rng: &mut Rng,
) -> Option<Act> {
    let mut act = None;
    // The bench: a floor grid with the tug turning slowly on it.
    let bw = 380.0;
    let pv = Rect::new(r.x, r.y, bw, 150.0);
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
        paint: profile.paint_colours(),
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
    let note_h = paragraph(
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
    if let Some(a) = paint_row(look, u, profile, r.x, sy + 4.0 + note_h + 8.0, bw) {
        act = Some(a);
    }

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
    if let Some(a) = sentry_row(look, u, career, x2, &mut y2, cw) {
        act = Some(a);
    }
    if let Some(a) = cores_row(look, u, career, x2, &mut y2, cw) {
        act = Some(a);
    }
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

/// The sentry drone row: needs a mounted drone bay with room (buySentry).
fn sentry_row(
    look: &Look,
    u: &mut Ui,
    career: &Career,
    x: f32,
    y: &mut f32,
    w: f32,
) -> Option<Act> {
    let bay = career.is_mounted(Part::DroneBay);
    let s = if bay {
        "Orbit the tug and shoot hostiles. Never the sheriff. Lost if destroyed."
    } else if career.owns(Part::DroneBay) {
        "Mount your drone bay to launch them."
    } else {
        "Needs a drone bay (utility part, below)."
    };
    let h = (44.0 + paragraph_h(look, s, w - 140.0, 14.0, Which::Body, 1.3)).max(70.0);
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
    let right = r.x + r.w - 12.0;
    status(look, u, &cr(SENTRY_PRICE), right, r.y + 12.0, col::HAZARD);
    let full = career.sentries >= SENTRY_MAX;
    let lbl = if full { "Bay full" } else { "Buy sentry" };
    let br = ui::button_rect(look, 0.0, 0.0, Style::Buy, lbl);
    let br = Rect::new(right - br.w, r.y + 34.0, br.w, br.h);
    let can = bay && !full && career.wallet >= SENTRY_PRICE;
    let pressed = u.button(look, "sentry", br, Style::Buy, lbl, can);
    *y += r.h + 6.0;
    pressed.then_some(Act::BuySentry)
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
        let free = career.free_slot(d.slot).is_some();
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

/* ------------------------ Jobs & crew, Map & standing ------------------------ */

/// One shop row (.sys) with a name, a line of detail, a status on the
/// right and one button under it. `picked` draws the cable border of an
/// accepted contract or a hired co-pilot. Returns whether it was pressed.
#[allow(clippy::too_many_arguments)]
fn offer_row(
    look: &Look,
    u: &mut Ui,
    id: &str,
    name: &str,
    detail: &str,
    status_s: (&str, u32),
    button: (&str, Style, bool),
    picked: bool,
    x: f32,
    y: &mut f32,
    w: f32,
) -> bool {
    let tw = w - 24.0 - 130.0;
    // Long contract names shrink a step rather than run under the button.
    let size = if text_w(look, name, 20.0, Which::Display) > tw {
        16.0
    } else {
        20.0
    };
    let h = (36.0 + paragraph_h(look, detail, tw, 14.0, Which::Body, 1.3) + 10.0).max(70.0);
    let r = Rect::new(x, *y, w, h);
    draw_rectangle(r.x, r.y, r.w, r.h, c(0x0b101c));
    let edge = if picked { col::CABLE } else { col::PANEL_EDGE };
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, c(edge));
    text(
        look,
        u,
        name,
        r.x + 12.0,
        r.y + 8.0,
        size,
        Which::Display,
        c(col::TEXT),
    );
    paragraph(
        look,
        u,
        detail,
        r.x + 12.0,
        r.y + 36.0,
        tw,
        14.0,
        Which::Body,
        c(col::MUTED),
        1.3,
    );
    let right = r.x + r.w - 12.0;
    status(look, u, status_s.0, right, r.y + 12.0, status_s.1);
    let (lbl, style, enabled) = button;
    let br = ui::button_rect(look, 0.0, 0.0, style, lbl);
    let br = Rect::new(right - br.w, r.y + 34.0, br.w, br.h);
    let pressed = u.button(look, id, br, style, lbl, enabled);
    *y += h + 6.0;
    pressed
}

/// The Jobs & crew tab (renderJobs): this shift's contract offers on the
/// left, the four co-pilots for hire on the right.
fn jobs_tab(look: &Look, u: &mut Ui, career: &Career, r: Rect) -> Option<Act> {
    let mut act = None;
    let cw = (r.w - 28.0) / 2.0;
    let (x1, x2) = (r.x, r.x + cw + 28.0);
    let (mut y1, mut y2) = (r.y, r.y);
    section(
        look,
        u,
        &format!("Contracts for shift {}", career.shift),
        x1,
        &mut y1,
    );
    for (i, k) in career.offers.iter().enumerate() {
        let picked = career.offer_taken(i);
        let detail = format!("Bonus {} if met by the end of the shift.", cr(k.reward));
        let st = if picked {
            ("Accepted".to_owned(), col::CABLE)
        } else {
            (format!("+{}", cr(k.reward)), col::HAZARD)
        };
        let lbl = if picked { "Drop" } else { "Accept" };
        if offer_row(
            look,
            u,
            &format!("job-{i}"),
            &k.text(),
            &detail,
            (&st.0, st.1),
            (lbl, Style::Buy, true),
            picked,
            x1,
            &mut y1,
            cw,
        ) {
            act = Some(Act::Offer(i));
        }
    }
    section(look, u, "Crew", x2, &mut y2);
    for m in CREWS {
        let d = m.def();
        let hired = career.crew == Some(m);
        let st = if hired {
            ("Aboard".to_owned(), col::CABLE)
        } else {
            (format!("{} / shift", cr(d.wage)), col::HAZARD)
        };
        let lbl = if hired {
            "Let go"
        } else if career.crew.is_some() {
            "Swap in"
        } else {
            "Hire"
        };
        if offer_row(
            look,
            u,
            &format!("crew-{}", d.key),
            &format!("{} · {}", d.name, d.role),
            d.perk,
            (&st.0, st.1),
            (lbl, Style::Buy, true),
            hired,
            x2,
            &mut y2,
            cw,
        ) {
            act = Some(Act::Crew(m));
        }
    }
    act
}

/// What a standing tier means in play (repEffects).
fn rep_effects(i: usize, v: f64) -> &'static str {
    let t = tier(v);
    match (i, t) {
        (0, Tier::Good) => "Installments 10% lower. Collectors give you until late in the shift.",
        (0, Tier::Bad) => "Installments 15% higher. Collectors move early and get greedy.",
        (0, _) => "Standard terms.",
        (1, Tier::Good) => "No shakedowns. Fines 30% lower.",
        (1, Tier::Bad) => "Shakedowns twice as likely, fines 30% higher, patrols more often.",
        (1, _) => "Routine patrols.",
        (_, Tier::Good) => "Fewer crews come hunting; grudges cool faster.",
        (_, Tier::Bad) => "Crews start every shift annoyed with you.",
        _ => "Business as usual.",
    }
}

/// The Map & standing tab: the sector map's belt cards (renderBelts) and
/// the three standings with the truce (renderStanding).
fn map_tab(look: &Look, u: &mut Ui, career: &Career, r: Rect) -> Option<Act> {
    let mut act = None;
    let cw = (r.w - 28.0) / 2.0;
    let (x1, x2) = (r.x, r.x + cw + 28.0);
    let (mut y1, mut y2) = (r.y, r.y);
    section(look, u, "Sector map", x1, &mut y1);
    // repeat(auto-fill, minmax(150px, 1fr)) with an 8 px gap; each grid
    // row is as tall as its tallest card.
    let gap = 8.0;
    let cols = ((cw + gap) / (150.0 + gap)).floor().max(1.0) as usize;
    let bw = (cw - gap * (cols as f32 - 1.0)) / cols as f32;
    let iw = bw - 20.0;
    let conds = career
        .conditions
        .filter(|cs| cs.0 == career.shift)
        .map(|cs| cs.1);
    let cards: Vec<(Belt, bool, String, Option<String>)> = BELTS
        .iter()
        .enumerate()
        .map(|(i, &b)| {
            let d = b.def();
            let locked = d.unlock > career.tug_class;
            let blurb = if locked {
                format!("Permit needs a {} tug.", TUG_CLASSES[d.unlock].mk)
            } else {
                d.blurb.to_owned()
            };
            let cond = conds
                .map(|m| m[i])
                .filter(|cd| !locked && *cd != Condition::Normal)
                .map(|cd| format!("This shift: {}. {}", cd.def().name, cd.def().text));
            (b, locked, blurb, cond)
        })
        .collect();
    let card_h = |c: &(Belt, bool, String, Option<String>)| {
        let mut h = 16.0 + 18.0 + 2.0 * 17.0;
        h += paragraph_h(look, &c.2, iw, 12.0, Which::Body, 1.3) + 3.0;
        if let Some(t) = &c.3 {
            h += paragraph_h(look, t, iw, 12.0, Which::Body, 1.3) + 3.0;
        }
        h
    };
    let mut gy = y1;
    for row in cards.chunks(cols) {
        let rh = row.iter().map(card_h).fold(0.0, f32::max);
        for (j, card) in row.iter().enumerate() {
            let (b, locked, blurb, cond) = card;
            let d = b.def();
            let bx = x1 + j as f32 * (bw + gap);
            let br = Rect::new(bx, gy, bw, rh);
            let picked = career.belt == *b;
            if u.button(
                look,
                &format!("belt-{}", d.key),
                br,
                Style::Belt { picked },
                "",
                !locked,
            ) {
                act = Some(Act::Belt(*b));
            }
            if picked {
                // .belt.picked: an inset 1 px ring inside the border.
                draw_rectangle_lines(
                    br.x + 1.0,
                    br.y + 1.0,
                    br.w - 2.0,
                    br.h - 2.0,
                    1.0,
                    c(col::CABLE),
                );
            }
            // .belt:disabled fades the whole card.
            let a = if *locked { 0.45 } else { 1.0 };
            let (ix, mut iy) = (bx + 10.0, gy + 8.0);
            text(
                look,
                u,
                d.name,
                ix,
                iy,
                15.0,
                Which::Body,
                rgba(col::TEXT, a),
            );
            iy += 21.0;
            for (lbl, n) in [("Salvage", d.salvage), ("Danger ", d.danger)] {
                let lw = text(
                    look,
                    u,
                    lbl,
                    ix,
                    iy,
                    11.0,
                    Which::MonoLight,
                    rgba(col::MUTED, a),
                );
                ui::pips(
                    ix + lw + 7.0,
                    iy + 7.0,
                    usize::from(n),
                    5,
                    11.0,
                    rgba(col::MUTED, a),
                );
                iy += 17.0;
            }
            iy += 3.0;
            iy += paragraph(
                look,
                u,
                blurb,
                ix,
                iy,
                iw,
                12.0,
                Which::Body,
                rgba(col::MUTED, a),
                1.3,
            ) + 3.0;
            if let Some(t) = cond {
                paragraph(
                    look,
                    u,
                    t,
                    ix,
                    iy,
                    iw,
                    12.0,
                    Which::Body,
                    c(col::HAZARD),
                    1.3,
                );
            }
        }
        gy += rh + gap;
    }

    section(look, u, "Standing", x2, &mut y2);
    let vals = [career.rep.bank, career.rep.law, career.rep.rivals];
    for (i, v) in vals.into_iter().enumerate() {
        let t = tier(v);
        // Index 2 is the rival crews; show what stolen cargo has bought them.
        let crews = if i == 2 && career.war_chest > 0.0 {
            format!(
                " War chest {} · crew level {}.",
                cr(career.war_chest),
                career.crew_level()
            )
        } else {
            String::new()
        };
        let detail = format!(
            "{}{} · {}{crews}",
            if v > 0.0 { "+" } else { "" },
            v,
            rep_effects(i, v)
        );
        let tw = cw - 24.0 - 130.0;
        let h = (36.0 + paragraph_h(look, &detail, tw, 14.0, Which::Body, 1.3) + 10.0).max(70.0);
        let rr = Rect::new(x2, y2, cw, h);
        draw_rectangle(rr.x, rr.y, rr.w, rr.h, c(0x0b101c));
        draw_rectangle_lines(rr.x, rr.y, rr.w, rr.h, 1.0, c(col::PANEL_EDGE));
        let nw = text(
            look,
            u,
            REP_NAMES[i],
            rr.x + 12.0,
            rr.y + 8.0,
            20.0,
            Which::Display,
            c(col::TEXT),
        );
        // .repbar: 120 x 6 with a centre tick; the fill grows from the
        // centre, left for bad standing and right for good.
        let (bx, by) = (rr.x + 12.0 + nw + 10.0, rr.y + 16.0);
        draw_rectangle(bx, by, 120.0, 6.0, c(0x0b101c));
        draw_rectangle_lines(bx, by, 120.0, 6.0, 1.0, c(col::PANEL_EDGE));
        let fill = match t {
            Tier::Good => col::CABLE,
            Tier::Bad => col::DANGER,
            Tier::Neutral => col::MUTED,
        };
        let left = 50.0 + v.min(0.0) as f32 / 2.0;
        draw_rectangle(
            bx + 1.2 * left,
            by,
            1.2 * v.abs() as f32 / 2.0,
            6.0,
            c(fill),
        );
        draw_line(bx + 60.0, by - 2.0, bx + 60.0, by + 8.0, 1.0, c(col::MUTED));
        paragraph(
            look,
            u,
            &detail,
            rr.x + 12.0,
            rr.y + 36.0,
            tw,
            14.0,
            Which::Body,
            c(col::MUTED),
            1.3,
        );
        if i == 2 {
            let right = rr.x + rr.w - 12.0;
            if truce_stage(career.grudge) > 0 {
                let price = career.truce_price();
                status(look, u, &cr(price), right, rr.y + 12.0, col::HAZARD);
                let lbl = "Buy truce";
                let br = ui::button_rect(look, 0.0, 0.0, Style::Buy, lbl);
                let br = Rect::new(right - br.w, rr.y + 34.0, br.w, br.h);
                if u.button(look, "truce", br, Style::Buy, lbl, career.wallet >= price) {
                    act = Some(Act::Truce);
                }
            } else {
                status(look, u, "No grudge", right, rr.y + 12.0, col::CABLE);
            }
        }
        y2 += h + 6.0;
    }
    act
}

/// Stored rogue cores (renderCores): sell one, or reprogram it into a sentry.
fn cores_row(look: &Look, u: &mut Ui, career: &Career, x: f32, y: &mut f32, w: f32) -> Option<Act> {
    if career.cores == 0 {
        return None;
    }
    let detail =
        "Sell one, or reprogram it into a sentry drone (needs a mounted drone bay with room).";
    let tw = w - 24.0 - 200.0;
    let h = (36.0 + paragraph_h(look, detail, tw, 14.0, Which::Body, 1.3) + 10.0).max(70.0);
    let r = Rect::new(x, *y, w, h);
    draw_rectangle(r.x, r.y, r.w, r.h, c(0x0b101c));
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, c(col::PANEL_EDGE));
    let name = format!("Rogue cores · {} stored", career.cores);
    text(
        look,
        u,
        &name,
        r.x + 12.0,
        r.y + 8.0,
        20.0,
        Which::Display,
        c(col::TEXT),
    );
    paragraph(
        look,
        u,
        detail,
        r.x + 12.0,
        r.y + 36.0,
        tw,
        14.0,
        Which::Body,
        c(col::MUTED),
        1.3,
    );
    let mut act = None;
    let right = r.x + r.w - 12.0;
    let prog = ui::button_rect(look, 0.0, 0.0, Style::Buy, "Reprogram");
    let prog = Rect::new(right - prog.w, r.y + 34.0, prog.w, prog.h);
    let can = career.is_mounted(Part::DroneBay) && career.sentries < SENTRY_MAX;
    if u.button(look, "core-prog", prog, Style::Buy, "Reprogram", can) {
        act = Some(Act::ReprogramCore);
    }
    let sell_l = format!("Sell {}", cr(CORE_SELL));
    let sell = ui::button_rect(look, 0.0, 0.0, Style::Act, &sell_l);
    let sell = Rect::new(prog.x - 6.0 - sell.w, prog.y, sell.w, sell.h);
    if u.button(look, "core-sell", sell, Style::Act, &sell_l, true) {
        act = Some(Act::SellCore);
    }
    *y += h + 6.0;
    act
}

/* ------------------------------ Marge ------------------------------ */

/// Marge between shifts (margeVerdict): one line on the shift just flown,
/// from the career's lastShift record.
fn marge_verdict(career: &Career) -> String {
    let Some(ls) = career.extra.get("lastShift").filter(|v| v.is_object()) else {
        return "First shift's on the house, hotshot. Well, it's on the loan. Everything is on the loan.".to_owned();
    };
    let num = |k: &str| ls.get(k).and_then(serde_json::Value::as_f64).unwrap_or(0.0);
    let truthy = |k: &str| match ls.get(k) {
        Some(serde_json::Value::Bool(b)) => *b,
        Some(serde_json::Value::Number(n)) => n.as_f64().is_some_and(|v| v != 0.0),
        Some(serde_json::Value::String(t)) => !t.is_empty(),
        _ => false,
    };
    let contract = ls.get("contract").and_then(serde_json::Value::as_str);
    if truthy("tradedUp") {
        "New tug, new loan, same pilot. The bank loves you. That is not a compliment.".to_owned()
    } else if num("wallet") < 0.0 {
        "You're in overdraft, hon. The bank calls that a relationship. I call it a leash."
            .to_owned()
    } else if num("deaths") >= 2.0 {
        format!(
            "{} wrecks in one shift. The insurance people know my voice now.",
            num("deaths")
        )
    } else if let Some(b) = ls
        .get("boss")
        .and_then(serde_json::Value::as_str)
        .filter(|b| !b.is_empty())
    {
        format!("You took down {b}. The other crews are being polite about you now. It's creepy.")
    } else if truthy("arrests") {
        "Spent part of your shift in cuffs. The fine came out of your cargo. Very efficient of you."
            .to_owned()
    } else if truthy("hold") {
        "You cracked a sealed hold. I didn't ask whose. I never ask whose.".to_owned()
    } else if num("earned") >= 6000.0 {
        "That's a real haul. I'm writing it in the logbook. In pencil.".to_owned()
    } else if contract == Some("done") {
        "Client is happy. Happy clients pay. Remember that feeling; it is rare.".to_owned()
    } else if contract == Some("missed") {
        "You missed the contract. The client called. I let it ring.".to_owned()
    } else if num("kills") >= 5.0 {
        format!(
            "{} crews scrapped. You're not a salvager, you're weather.",
            num("kills")
        )
    } else if truthy("escort") {
        "The hauler made it. Their captain sent flowers. I ate them. Long shift.".to_owned()
    } else if num("earned") < 500.0 {
        "That was a shift. Technically.".to_owned()
    } else {
        "Decent shift. Not great. Decent. Like my coffee.".to_owned()
    }
}

/// The shop's Marge box (#shop-marge): her portrait and her verdict.
fn marge_box(look: &Look, u: &Ui, career: &Career, r: Rect) {
    draw_rectangle(r.x, r.y, r.w, r.h, c(0x0c1220));
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, c(col::PANEL_EDGE));
    let face = Rect::new(r.x + 10.0, r.y + 6.0, 52.0, 52.0);
    draw_marge(face);
    let tx = face.x + face.w + 12.0;
    let tw = r.x + r.w - 10.0 - tx;
    label(look, u, "Marge · dispatch", tx, r.y + 8.0, c(col::MUTED));
    let line = marge_verdict(career);
    let size = if paragraph_h(look, &line, tw, 14.0, Which::Body, 1.4) > 40.0 {
        12.5
    } else {
        14.0
    };
    paragraph(
        look,
        u,
        &line,
        tx,
        r.y + 24.0,
        tw,
        size,
        Which::Body,
        c(col::TEXT),
        1.4,
    );
}

/// drawMarge: grey bun, cat-eye glasses, a headset with a boom mic, and
/// the mug, in a square `r`.
fn draw_marge(r: Rect) {
    let (x0, y0, w, h) = (r.x, r.y, r.w, r.h);
    let p = |fx: f32, fy: f32| vec2(x0 + w * fx, y0 + h * fy);
    draw_rectangle(x0, y0, w, h, c(0x0c1220));
    let hair = c(0x9aa0a8);
    draw_circle(x0 + w * 0.5, y0 + h * 0.2, h * 0.13, hair); // bun
                                                             // Hair: the top half of a circle.
    let (cx, cy, hr) = (x0 + w * 0.5, y0 + h * 0.42, h * 0.27);
    for i in 0..24 {
        let a0 = std::f32::consts::PI * (1.0 + i as f32 / 24.0);
        let a1 = std::f32::consts::PI * (1.0 + (i + 1) as f32 / 24.0);
        draw_triangle(
            vec2(cx, cy),
            vec2(cx + hr * a0.cos(), cy + hr * a0.sin()),
            vec2(cx + hr * a1.cos(), cy + hr * a1.sin()),
            hair,
        );
    }
    draw_ellipse(
        x0 + w * 0.5,
        y0 + h * 0.52,
        h * 0.22,
        h * 0.26,
        0.0,
        c(0xc99d78),
    ); // face
    let ink = c(0x1b1b22);
    for sx in [-1.0f32, 1.0] {
        // Cat-eye glasses.
        let q = [
            p(0.5 + sx * 0.02, 0.47),
            p(0.5 + sx * 0.2, 0.44),
            p(0.5 + sx * 0.18, 0.53),
            p(0.5 + sx * 0.04, 0.53),
        ];
        for k in 0..4 {
            draw_line(q[k].x, q[k].y, q[(k + 1) % 4].x, q[(k + 1) % 4].y, 2.0, ink);
        }
    }
    let quad = |a: Vec2, b: Vec2, e: Vec2, width: f32, colr: Color| {
        let mut prev = a;
        for i in 1..=12 {
            let t = i as f32 / 12.0;
            let q = a * (1.0 - t) * (1.0 - t) + b * 2.0 * t * (1.0 - t) + e * t * t;
            draw_line(prev.x, prev.y, q.x, q.y, width, colr);
            prev = q;
        }
    };
    quad(p(0.42, 0.66), p(0.52, 0.69), p(0.6, 0.63), 2.0, ink); // smirk
    let hz = c(col::HAZARD);
    // Headset band.
    let (bx, by, br) = (x0 + w * 0.5, y0 + h * 0.5, h * 0.3);
    let mut prev = vec2(
        bx + br * (std::f32::consts::PI * 1.05).cos(),
        by + br * (std::f32::consts::PI * 1.05).sin(),
    );
    for i in 1..=16 {
        let a = std::f32::consts::PI * (1.05 + 0.9 * i as f32 / 16.0);
        let q = vec2(bx + br * a.cos(), by + br * a.sin());
        draw_line(prev.x, prev.y, q.x, q.y, 2.5, hz);
        prev = q;
    }
    quad(p(0.23, 0.55), p(0.28, 0.78), p(0.44, 0.74), 2.5, hz); // boom mic
    draw_circle(x0 + w * 0.45, y0 + h * 0.74, 2.2, hz);
    draw_rectangle(
        x0 + w * 0.66,
        y0 + h * 0.78,
        h * 0.16,
        h * 0.16,
        c(0xd24b3c),
    ); // the mug
    draw_rectangle_lines(x0, y0, w, h, 1.0, hz);
}

/* ------------------------- Settings, records, board ------------------------- */

/// What the settings screen shows: the settings, the key being rebound (if any) and a status message.
pub struct SettingsView<'a> {
    pub settings: &'a crate::settings::Settings,
    pub rebinding: Option<crate::settings::Bind>,
    pub msg: &'a str,
}

/// The settings screen (#screen-settings): toggles on the left, the key
/// map on the right.
pub fn settings(
    look: &Look,
    u: &mut Ui,
    input: &MenuInput,
    view: (f32, f32),
    dpi: f32,
    v: SettingsView,
) -> Option<Act> {
    use crate::settings::{key_label, KEY_ACTIONS};
    let (w, h) = (1040.0, 600.0);
    u.begin(input, (w, h), view, dpi);
    let r = Rect::new(20.0, 20.0, 1000.0, 560.0);
    card(r);
    let (x, mut y) = (r.x + 36.0, r.y + 34.0);
    label(look, u, "Hollis-3 · cockpit settings", x, y, c(col::MUTED));
    y += 22.0;
    text(
        look,
        u,
        "Settings",
        x,
        y,
        56.0,
        Which::Display,
        c(col::HAZARD),
    );
    y += 72.0;
    let s = v.settings;
    let mut act = None;
    let onoff = |b: bool| if b { "on" } else { "off" };
    let toggles = [
        (
            "set-music",
            format!("Music: {}", onoff(s.music)),
            Act::SetMusic,
        ),
        (
            "set-motion",
            format!("Reduce shake and flashing: {}", onoff(s.reduce_motion)),
            Act::SetMotion,
        ),
        (
            "set-palette",
            format!("Colorblind-safe palette: {}", onoff(s.colorblind)),
            Act::SetPalette,
        ),
        (
            "set-effects",
            format!("Glow and effects: {}", onoff(s.effects)),
            Act::SetEffects,
        ),
        (
            "set-sprites",
            format!(
                "Art style: {}",
                if s.sprites { "painted" } else { "vector" }
            ),
            Act::SetSprites,
        ),
        (
            "set-hud",
            format!("HUD size: {}%", s.hud_pct),
            Act::CycleHud,
        ),
        (
            "set-coop",
            format!("Local co-op (P2): {}", s.coop.label()),
            Act::CycleCoop,
        ),
        (
            "set-voice",
            format!("Marge's voice: {}", onoff(s.voice)),
            Act::ToggleVoice,
        ),
    ];
    let mut ty = y;
    for (id, lbl, a) in toggles {
        let (p, br) = u.button_at(look, id, x, ty, Style::Ghost, &lbl, true);
        if p {
            act = Some(a);
        }
        // Tight spacing: eight rows must clear the Back button.
        ty += br.h + 4.0;
    }
    // Sound: the four volumes, between the toggles and the key map.
    let (sx, mut sy) = (r.x + 330.0, y);
    section(look, u, "Sound", sx, &mut sy);
    for (k, (name, _)) in crate::settings::VOLUMES.iter().enumerate() {
        let id = format!("set-vol-{k}");
        let lbl = format!("{name}: {}%", s.volume[k]);
        let (p, br) = u.button_at(look, &id, sx, sy, Style::Ghost, &lbl, true);
        if p {
            act = Some(Act::CycleVolume(k));
        }
        sy += br.h + 4.0;
    }
    let kx = r.x + 520.0;
    let kw = r.x + r.w - 36.0 - kx;
    section(look, u, "Controls", kx, &mut y);
    let help = "Select an action, then press the new key. Esc cancels. P, M, V, N, T and Enter are reserved.";
    y += paragraph(
        look,
        u,
        help,
        kx,
        y,
        kw,
        12.0,
        Which::MonoLight,
        c(col::MUTED),
        1.4,
    ) + 8.0;
    let col_w = (kw - 8.0) / 2.0;
    for (i, (b, id, lbl, _)) in KEY_ACTIONS.iter().enumerate() {
        let bx = kx + (i % 2) as f32 * (col_w + 8.0);
        let by = y + (i / 2) as f32 * 40.0;
        let br = Rect::new(bx, by, col_w, 34.0);
        let listening = v.rebinding == Some(*b);
        let keys = if listening {
            "press a key".to_owned()
        } else {
            s.codes_for(*b)
                .iter()
                .map(|c| key_label(c))
                .collect::<Vec<_>>()
                .join(" / ")
        };
        if u.button(look, &format!("bind-{id}"), br, Style::Ghost, "", true) {
            act = Some(Act::Rebind(*b));
        }
        if listening {
            draw_rectangle_lines(br.x, br.y, br.w, br.h, 2.0, c(col::HAZARD));
        }
        text(
            look,
            u,
            lbl,
            br.x + 10.0,
            br.y + 9.0,
            13.0,
            Which::Mono,
            c(col::TEXT),
        );
        let kwid = text_w(look, &keys, 13.0, Which::Mono);
        let kc = if listening { col::HAZARD } else { col::CABLE };
        text(
            look,
            u,
            &keys,
            br.x + br.w - 10.0 - kwid,
            br.y + 9.0,
            13.0,
            Which::Mono,
            c(kc),
        );
    }
    let my = y + 6.0 * 40.0 + 4.0;
    paragraph(
        look,
        u,
        v.msg,
        kx,
        my,
        kw,
        12.0,
        Which::MonoLight,
        c(col::CABLE),
        1.4,
    );
    // The co-op note sits under the rebind message, clear of the rows.
    let note = "Co-op: P2 flies a partner tug from the numpad (8 thrust, 4/6 turn, 5 brake, 0 fire, Enter latch) or the last connected gamepad (stick, RT, LT, RB, Y latch). Latch onto the tail of the cable to haul together.";
    paragraph(
        look,
        u,
        note,
        kx,
        my + 44.0,
        kw,
        12.0,
        Which::MonoLight,
        c(col::MUTED),
        1.4,
    );

    let ay = r.y + r.h - 70.0;
    let (p, br) = u.button_at(look, "back", x, ay, Style::Primary, "Back", true);
    if p || input.back {
        act = Some(Act::Back);
    }
    let by = ay + (br.h - 37.0) / 2.0;
    if u.button_at(
        look,
        "keys-reset",
        br.x + br.w + 12.0,
        by,
        Style::Ghost,
        "Reset controls",
        true,
    )
    .0
    {
        act = Some(Act::ResetKeys);
    }
    act
}

/// Records (#screen-records): lifetime stats, the kill tally, and the
/// achievements with how to earn each.
pub fn records(
    look: &Look,
    u: &mut Ui,
    input: &MenuInput,
    view: (f32, f32),
    dpi: f32,
    p: &crate::profile::Profile,
) -> Option<Act> {
    use crate::profile::{ACHIEVEMENTS, KILL_KINDS, PROFILE_STATS};
    let (w, h) = (1040.0, 660.0);
    u.begin(input, (w, h), view, dpi);
    let r = Rect::new(20.0, 20.0, 1000.0, 620.0);
    card(r);
    let (x, mut y) = (r.x + 36.0, r.y + 34.0);
    label(
        look,
        u,
        "Across every career on this PC",
        x,
        y,
        c(col::MUTED),
    );
    y += 22.0;
    text(
        look,
        u,
        "Records",
        x,
        y,
        56.0,
        Which::Display,
        c(col::HAZARD),
    );
    y += 72.0;
    let stats: Vec<(&str, String, u32)> = PROFILE_STATS
        .iter()
        .zip(p.stats)
        .map(|((_, l), v)| (*l, fmt_credits(v), col::TEXT))
        .collect();
    let (_, sh) = ui::ledger(look, u, x, y, &stats);
    let mut ky = y + sh + 14.0;
    section(look, u, "Kill tally", x, &mut ky);
    let kills: Vec<(&str, String, u32)> = KILL_KINDS
        .iter()
        .zip(p.kills)
        .map(|((_, l), v)| (*l, fmt_credits(f64::from(v)), col::TEXT))
        .collect();
    ui::ledger(look, u, x, ky, &kills);

    let ax = r.x + 480.0;
    let aw = r.x + r.w - 36.0 - ax;
    let got = ACHIEVEMENTS.iter().filter(|a| p.has(a.0)).count();
    let mut ay = y;
    section(
        look,
        u,
        &format!("Achievements {got} / {}", ACHIEVEMENTS.len()),
        ax,
        &mut ay,
    );
    for (id, title, how) in ACHIEVEMENTS {
        let has = p.has(id);
        let t = if has {
            title.to_owned()
        } else {
            format!("{title} (locked)")
        };
        text(
            look,
            u,
            &t,
            ax,
            ay,
            15.0,
            Which::Body,
            c(if has { col::HAZARD } else { col::MUTED }),
        );
        ay += 18.0;
        text(look, u, how, ax, ay, 12.0, Which::Body, c(col::MUTED));
        ay += 18.0;
    }
    let _ = aw;
    let by = r.y + r.h - 70.0;
    let (pr, _) = u.button_at(look, "back", x, by, Style::Primary, "Back", true);
    (pr || input.back).then_some(Act::Back)
}

/// What the leaderboard shows: the score rows, the selected tab, the pilot's name and today's date.
pub struct BoardView<'a> {
    pub rows: &'a [crate::profile::Score],
    /// 0..3 the difficulties, 3 the daily challenge.
    pub tab: usize,
    pub pilot: &'a str,
    pub day: &'a str,
}

/// The leaderboard (#screen-board): this PC's best shifts per difficulty,
/// and today's daily challenge.
pub fn board(
    look: &Look,
    u: &mut Ui,
    input: &MenuInput,
    view: (f32, f32),
    dpi: f32,
    v: BoardView,
) -> Option<Act> {
    let (w, h) = (760.0, 660.0);
    u.begin(input, (w, h), view, dpi);
    let r = Rect::new(20.0, 20.0, 720.0, 620.0);
    card(r);
    let (x, mut y) = (r.x + 36.0, r.y + 34.0);
    label(
        look,
        u,
        "Hollis Belt · best single shift",
        x,
        y,
        c(col::MUTED),
    );
    y += 22.0;
    text(
        look,
        u,
        "Leaderboard",
        x,
        y,
        56.0,
        Which::Display,
        c(col::HAZARD),
    );
    y += 72.0;
    let mut act = None;
    let names = ["Relaxed", "Standard", "Hard", "Daily"];
    let bw = 160.0;
    for (i, n) in names.iter().enumerate() {
        let br = Rect::new(x + i as f32 * bw, y, bw, 38.0);
        if u.button(
            look,
            &format!("board-{i}"),
            br,
            Style::Mode {
                pressed: v.tab == i,
            },
            n,
            true,
        ) {
            act = Some(Act::BoardTab(i));
        }
    }
    y += 50.0;
    let status = format!(
        "Scores on this PC. Your best runs post here as {}.",
        v.pilot
    );
    text(
        look,
        u,
        &status,
        x,
        y,
        12.0,
        Which::MonoLight,
        c(col::MUTED),
    );
    y += 24.0;
    if v.rows.is_empty() {
        let empty = if v.tab == 3 {
            format!(
                "No daily runs for {} yet. Fly today's challenge to set the first one.",
                v.day
            )
        } else {
            format!(
                "No {} runs yet. Finish a shift to set the first one.",
                names[v.tab]
            )
        };
        text(look, u, &empty, x, y, 15.0, Which::Body, c(col::MUTED));
    }
    let lw = r.w - 72.0;
    for (i, row) in v.rows.iter().take(20).enumerate() {
        let me = row.name == v.pilot;
        let ry = y + i as f32 * 21.0;
        let fg = c(if me { col::HAZARD } else { col::TEXT });
        text(
            look,
            u,
            &format!("{}", i + 1),
            x,
            ry,
            13.0,
            Which::Mono,
            c(col::MUTED),
        );
        let who = if me {
            format!("{} (you)", row.name)
        } else {
            row.name.clone()
        };
        text(look, u, &who, x + 36.0, ry, 13.0, Which::Mono, fg);
        if row.shift > 0 {
            let m = format!("shift {}", row.shift);
            text(
                look,
                u,
                &m,
                x + 330.0,
                ry,
                12.0,
                Which::MonoLight,
                c(col::MUTED),
            );
        }
        let pts = cr(row.score);
        let pw = text_w(look, &pts, 13.0, Which::Mono);
        text(look, u, &pts, x + lw - pw, ry, 13.0, Which::Mono, fg);
    }
    let by = r.y + r.h - 70.0;
    let (pr, _) = u.button_at(look, "back", x, by, Style::Primary, "Back", true);
    if pr || input.back {
        act = Some(Act::Back);
    }
    act
}

/// The sound test (#screen-sounds).
#[allow(clippy::too_many_arguments)]
pub fn sounds(
    look: &Look,
    u: &mut Ui,
    input: &MenuInput,
    view: (f32, f32),
    dpi: f32,
    groups: &[(&str, Vec<crate::sounds::Item>)],
    lit: Option<usize>,
    now_playing: &str,
) -> Option<Act> {
    let (w, h) = (1240.0, 720.0);
    u.begin(input, (w, h), view, dpi);
    let r = Rect::new(20.0, 20.0, 1200.0, 680.0);
    card(r);
    let (x, mut y) = (r.x + 30.0, r.y + 26.0);
    label(look, u, "Hollis-3 audio bench", x, y, c(col::MUTED));
    y += 20.0;
    text(
        look,
        u,
        "Sound test",
        x,
        y,
        48.0,
        Which::Display,
        c(col::HAZARD),
    );
    y += 58.0;
    let help = "Every sound in the game. Select one to hear it, or play them all in order. Mind your volume; the alarms are alarms.";
    text(look, u, help, x, y, 13.0, Which::Body, c(col::MUTED));
    y += 26.0;
    let mut act = None;
    // Groups in three columns.
    let cw = (r.w - 60.0 - 2.0 * 16.0) / 3.0;
    let mut cols = [y; 3];
    let mut flat = 0;
    for (gi, (name, items)) in groups.iter().enumerate() {
        let ci = (0..3)
            .min_by(|a, b| cols[*a].total_cmp(&cols[*b]))
            .unwrap_or(gi % 3);
        let gx = x + ci as f32 * (cw + 16.0);
        let gy = &mut cols[ci];
        section(look, u, name, gx, gy);
        for (i, item) in items.iter().enumerate() {
            let bx = gx + (i % 2) as f32 * (cw / 2.0 + 2.0);
            let br = Rect::new(bx, *gy, cw / 2.0 - 2.0, 26.0);
            let on = lit == Some(flat);
            if u.button(
                look,
                &format!("snd-{flat}"),
                br,
                Style::Slot { filled: on },
                "",
                true,
            ) {
                act = Some(Act::PlaySound(flat));
            }
            text(
                look,
                u,
                item.label,
                br.x + 8.0,
                br.y + 6.0,
                12.0,
                Which::Body,
                c(if on { col::HAZARD } else { col::TEXT }),
            );
            if i % 2 == 1 || i + 1 == items.len() {
                *gy += 30.0;
            }
            flat += 1;
        }
        *gy += 8.0;
    }
    let ay = r.y + r.h - 64.0;
    let (p, br) = u.button_at(look, "all", x, ay, Style::Primary, "Play all", true);
    if p {
        act = Some(Act::PlayAll);
    }
    let by = ay + (br.h - 37.0) / 2.0;
    let (p, sr) = u.button_at(
        look,
        "stop",
        br.x + br.w + 12.0,
        by,
        Style::Ghost,
        "Stop",
        true,
    );
    if p {
        act = Some(Act::StopSounds);
    }
    let (p, bk) = u.button_at(
        look,
        "back",
        sr.x + sr.w + 12.0,
        by,
        Style::Ghost,
        "Back",
        true,
    );
    if p || input.back {
        act = Some(Act::Back);
    }
    if !now_playing.is_empty() {
        let s = format!("Now playing: {now_playing}");
        text(
            look,
            u,
            &s,
            bk.x + bk.w + 16.0,
            by + 10.0,
            12.0,
            Which::MonoLight,
            c(col::CABLE),
        );
    }
    act
}

/// The Tug tab's paint picker (renderPaint): each job unlocks with an
/// achievement; the choice follows the pilot across careers.
fn paint_row(
    look: &Look,
    u: &mut Ui,
    p: &crate::profile::Profile,
    x: f32,
    y: f32,
    w: f32,
) -> Option<Act> {
    use crate::profile::PAINTS;
    let mut yy = y;
    section(look, u, "Paint", x, &mut yy);
    let mut act = None;
    let bw = (w - 6.0) / 2.0;
    for (i, (id, name, hull, trim, needs)) in PAINTS.iter().enumerate() {
        let open = needs.is_none_or(|a| p.has(a));
        let picked = p.paint == *id;
        let br = Rect::new(
            x + (i % 2) as f32 * (bw + 6.0),
            yy + (i / 2) as f32 * 24.0,
            bw,
            21.0,
        );
        if u.button(
            look,
            &format!("paint-{id}"),
            br,
            Style::Belt { picked },
            "",
            open,
        ) {
            act = Some(Act::Paint(i));
        }
        let a = if open { 1.0 } else { 0.45 };
        if *hull != 0 {
            draw_rectangle(br.x + 6.0, br.y + 5.0, 11.0, 11.0, rgba(*hull, a));
            draw_rectangle_lines(br.x + 6.0, br.y + 5.0, 11.0, 11.0, 1.0, rgba(*trim, a));
        }
        let lbl = if open {
            (*name).to_owned()
        } else {
            format!("{name} (locked)")
        };
        text(
            look,
            u,
            &lbl,
            br.x + 22.0,
            br.y + 4.0,
            11.0,
            Which::Body,
            rgba(col::TEXT, a),
        );
    }
    act
}
