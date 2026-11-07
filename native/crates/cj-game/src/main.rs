//! Claim Jumpers, native Windows build. Milestone 5: the career's field
//! systems. Fly the tug, tow and fight through five-minute shifts against
//! rival crews, debt collectors, the law, mining drones and the Kessler
//! refinery, on the belt you chose and under its conditions, with field
//! events, boss crews, escorts and sealed holds; after each, the shift
//! report settles the loan installment, the contract and the crew's wage,
//! and the machine shop sells parts, system levels, contracts, crew,
//! permits and truces. The career is saved in %APPDATA%\ClaimJumpers.
//!
//! The app moves between the browser's screens (appState in
//! src/js/14-game-state.js): title, play, paused, the shift report and the
//! shop. Behind every menu the field keeps drifting.
//!
//! `--smoke` runs an unattended check through every screen: a scripted
//! shift (tow and firefight, one simulation step per frame), the pause
//! card, the report, a purchase in the shop, each shop tab, and the title,
//! saving a screenshot of each to %TEMP%\claim-jumpers-m5-<screen>.png and printing
//! a report. It uses a temporary save folder, never the player's.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod controls;
mod draw;
mod foes;
mod fx;
mod hud;
mod look;
mod radar;
mod save;
mod screens;
mod tug;
mod ui;

use cj_sim::career::{Career, Mode, Outcome, Settlement};
use cj_sim::consts::{Variant, BAY_X, BAY_Y, MAX_FRAME_DT, MAX_STEPS_PER_FRAME, STEP};
use cj_sim::events::{Event, Sfx};
use cj_sim::items::ItemType;
use cj_sim::parts::Part;
use cj_sim::rng::Rng;
use cj_sim::{Action, Input, World};
use macroquad::prelude::*;
use screens::{Act, Report};

fn window_conf() -> Conf {
    Conf {
        window_title: "Claim Jumpers".to_owned(),
        window_width: 1280,
        window_height: 720,
        high_dpi: true,
        sample_count: 4,
        window_resizable: true,
        ..Default::default()
    }
}

/// Logical window size (macroquad already divides out the DPI scale).
fn view() -> (f32, f32) {
    (screen_width(), screen_height())
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Screen {
    Title,
    Play,
    Paused,
    Report,
    Shop,
}

/// Everything the game keeps between frames.
struct App {
    store: save::Store,
    bests: save::Bests,
    /// Difficulty picked for the next new career.
    mode: Mode,
    /// The career being played (None on the title screen).
    career: Option<Career>,
    /// The saved career, as the title screen describes it.
    saved: Option<Career>,
    world: World,
    fx: fx::Fx,
    screen: Screen,
    ui: ui::Ui,
    report: Option<Report>,
    title_tab: usize,
    shop_tab: usize,
    shop_msg: String,
    /// Two-press confirmations: erase the save, quit a shift to the menu.
    armed_new: bool,
    armed_menu: bool,
    accumulator: f64,
    seeded: bool,
    quit: bool,
}

/// True when the career goes on after this report (to the shop).
fn continues(rep: Option<&Report>) -> bool {
    matches!(
        rep.map(|r| &r.settlement.outcome),
        Some(Outcome::Continue | Outcome::TradedUp { .. })
    )
}

impl App {
    fn rng(&self) -> Rng {
        if self.seeded {
            Rng::seeded(7)
        } else {
            Rng::from_time()
        }
    }

    fn open(&mut self, s: Screen) {
        self.screen = s;
        self.ui.reset();
        self.armed_new = false;
        self.armed_menu = false;
        if s == Screen::Title {
            self.saved = self.store.load_career();
        }
        if s == Screen::Shop {
            let mut rng = self.rng();
            if let Some(c) = self.career.as_mut() {
                c.ensure_offers(&mut rng);
                c.ensure_conditions(&mut rng);
            }
        }
    }

    /// startShift: a new world for the career's next shift.
    fn start_shift(&mut self, audio: &mut audio::Audio) {
        if self.career.is_none() {
            let c = Career::new(self.mode);
            self.store.save_career(&c);
            self.career = Some(c);
        }
        let c = self.career.as_ref().expect("a career");
        self.world = World::for_career(self.rng(), self.world.view, c);
        self.fx = fx::Fx::new(self.rng());
        self.accumulator = 0.0;
        audio.set_thrust(0.0, 0.0, false);
        self.open(Screen::Play);
    }

    /// endShift: settle the career and show the report.
    fn end_shift(&mut self, audio: &mut audio::Audio) {
        let Some(c) = self.career.as_mut() else {
            return;
        };
        let mode = c.mode;
        let st: Settlement = c.settle(&self.world);
        let best = self.bests.get(mode).unwrap_or(0.0);
        let is_best = st.earned > best;
        if is_best {
            self.bests.set(mode, st.earned);
            self.store.save_bests(&self.bests);
        }
        match st.outcome {
            Outcome::Continue | Outcome::TradedUp { .. } => self.store.save_career(c),
            // A repossession or a paid-off fleet ends the career.
            Outcome::Repossessed { .. } | Outcome::FleetPaid => self.store.clear_career(),
        }
        let best = self.bests.get(mode).unwrap_or(0.0);
        self.report = Some(Report::new(st, &self.world, c, best, is_best));
        audio.set_thrust(0.0, 0.0, false);
        self.open(Screen::Report);
    }

    /// The loan was paid off from the shop: the report screen shows the
    /// trade-up (or the won career), as showPaidOff does.
    fn paid_off_in_shop(&mut self, outcome: Outcome) {
        let c = self.career.as_ref().expect("a career");
        if outcome == Outcome::FleetPaid {
            self.store.clear_career();
        } else {
            self.store.save_career(c);
        }
        let mut rep = match self.report.take() {
            Some(r) => r,
            None => Report::new(
                Settlement {
                    earned: 0.0,
                    due: 0.0,
                    contract: None,
                    wage: 0.0,
                    chain_note: String::new(),
                    rep_delta: [0.0; 3],
                    outcome: Outcome::Continue,
                },
                &self.world,
                c,
                self.bests.get(c.mode).unwrap_or(0.0),
                false,
            ),
        };
        rep.settlement.outcome = outcome;
        rep.career = c.clone();
        self.report = Some(rep);
        self.open(Screen::Report);
    }

    /// goToTitle: the career is already saved, so drop the in-memory copy;
    /// a fresh field drifts behind the title card.
    fn go_to_title(&mut self) {
        self.career = None;
        self.report = None;
        self.world = World::new_shift(self.rng(), self.world.view);
        self.fx = fx::Fx::new(self.rng());
        self.open(Screen::Title);
    }

    fn act(&mut self, a: Act, audio: &mut audio::Audio) {
        match a {
            Act::PickMode(m) => {
                self.mode = m;
                self.store.save_mode(m);
                self.armed_new = false;
            }
            Act::StartOrContinue => match self.store.load_career() {
                Some(c) => {
                    let to_shop = c.shift > 1;
                    self.career = Some(c);
                    if to_shop {
                        self.shop_msg.clear();
                        self.open(Screen::Shop);
                    } else {
                        self.start_shift(audio);
                    }
                }
                None => {
                    self.career = None;
                    self.start_shift(audio);
                }
            },
            Act::NewCareer => {
                if !self.armed_new {
                    self.armed_new = true;
                    return;
                }
                self.store.clear_career();
                let c = Career::new(self.mode);
                self.store.save_career(&c);
                self.career = Some(c);
                self.start_shift(audio);
            }
            Act::Quit => self.quit = true,
            Act::Resume => self.open(Screen::Play),
            Act::EndShiftNow => self.end_shift(audio),
            Act::PauseToMenu => {
                // Quitting mid-shift must not dodge the installment: it ends
                // the shift (two presses). A repossession or payoff stays on
                // the report so the player sees it.
                if !self.armed_menu {
                    self.armed_menu = true;
                    return;
                }
                self.end_shift(audio);
                if continues(self.report.as_ref()) {
                    self.go_to_title();
                }
            }
            Act::ReportNext => {
                if continues(self.report.as_ref()) {
                    self.shop_msg.clear();
                    self.open(Screen::Shop);
                } else {
                    // A fresh career keeps the mode you were playing.
                    let mode = self.career.as_ref().map_or(self.mode, |c| c.mode);
                    let c = Career::new(mode);
                    self.store.save_career(&c);
                    self.career = Some(c);
                    self.start_shift(audio);
                }
            }
            Act::ToMenu => self.go_to_title(),
            Act::StartShift => self.start_shift(audio),
            Act::Pay(limit) => {
                let c = self.career.as_mut().expect("a career");
                match c.pay_extra(limit) {
                    Ok((msg, None)) => {
                        audio.sfx(Sfx::Bank);
                        self.store.save_career(c);
                        self.shop_msg = msg;
                    }
                    Ok((_, Some(outcome))) => {
                        audio.sfx(Sfx::Bank);
                        self.paid_off_in_shop(outcome);
                    }
                    Err(msg) => self.shop_msg = msg,
                }
            }
            Act::Buy(p) | Act::Mount(p) | Act::Unmount(p) => {
                let c = self.career.as_mut().expect("a career");
                let (res, sound) = match a {
                    Act::Buy(_) => (c.buy_part(p), Sfx::Bank),
                    Act::Mount(_) => (c.mount(p), Sfx::Hook),
                    _ => (c.unmount(p), Sfx::Drop),
                };
                self.finish_shop(res, sound, audio);
            }
            Act::BuySentry => {
                let c = self.career.as_mut().expect("a career");
                let res = c.buy_sentry();
                self.finish_shop(res, Sfx::Bank, audio);
            }
            Act::Level(s) => {
                let c = self.career.as_mut().expect("a career");
                let res = c.buy_level(s);
                self.finish_shop(res, Sfx::Bank, audio);
            }
            Act::Offer(i) => {
                let c = self.career.as_mut().expect("a career");
                let res = Ok(c.toggle_offer(i));
                self.finish_shop(res, Sfx::Hook, audio);
            }
            Act::Crew(m) => {
                let c = self.career.as_mut().expect("a career");
                let res = Ok(c.toggle_crew(m));
                self.finish_shop(res, Sfx::Hook, audio);
            }
            Act::Belt(b) => {
                let c = self.career.as_mut().expect("a career");
                let res = c.pick_belt(b);
                self.finish_shop(res, Sfx::Hook, audio);
            }
            Act::Truce => {
                let c = self.career.as_mut().expect("a career");
                let res = c.buy_truce();
                self.finish_shop(res, Sfx::Bank, audio);
            }
            Act::SellCore => {
                let c = self.career.as_mut().expect("a career");
                let res = c.sell_core();
                self.finish_shop(res, Sfx::Bank, audio);
            }
            Act::ReprogramCore => {
                let c = self.career.as_mut().expect("a career");
                let res = c.reprogram_core();
                self.finish_shop(res, Sfx::Bank, audio);
            }
        }
    }

    fn finish_shop(&mut self, res: Result<String, String>, sound: Sfx, audio: &mut audio::Audio) {
        match res {
            Ok(msg) => {
                audio.sfx(sound);
                self.store
                    .save_career(self.career.as_ref().expect("a career"));
                self.shop_msg = msg;
            }
            Err(msg) => self.shop_msg = msg,
        }
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    let smoke = std::env::args().any(|a| a == "--smoke");
    if smoke {
        // A check run gets its own empty save folder.
        let dir = std::env::temp_dir().join(format!("claim-jumpers-smoke-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::env::set_var("CLAIM_JUMPERS_DATA", &dir);
    }
    let seeded = |s: u32| {
        if smoke {
            Rng::seeded(s)
        } else {
            Rng::from_time()
        }
    };
    let look = look::Look::new();
    let mut audio = audio::Audio::start();
    if let Some(e) = &audio.error {
        eprintln!("sound is off: {e}");
    }
    let mut controls = controls::Controls::new();
    let mut fx_rng = seeded(7);
    let stars = draw::Stars::new(&mut fx_rng);
    let mut vignette = draw::Vignette::new();
    let store = save::Store::open();
    let (vw, vh) = view();
    let mut app = App {
        bests: store.load_bests(),
        mode: store.load_mode(),
        saved: store.load_career(),
        store,
        career: None,
        world: World::new_shift(seeded(7), (f64::from(vw), f64::from(vh))),
        fx: fx::Fx::new(seeded(7)),
        screen: Screen::Title,
        ui: ui::Ui::new(),
        report: None,
        title_tab: 0,
        shop_tab: 0,
        shop_msg: String::new(),
        armed_new: false,
        armed_menu: false,
        accumulator: 0.0,
        seeded: smoke,
        quit: false,
    };
    let mut sm = Smoke::default();
    if smoke {
        app.act(Act::StartOrContinue, &mut audio);
    }
    let start = get_time();

    loop {
        controls.poll();
        let (vw, vh) = view();
        app.world.view = (f64::from(vw), f64::from(vh));
        // Both readers run every frame so their button edges stay current.
        let presses = controls.presses();
        let keys = controls.menu();
        if presses.mute {
            audio.toggle_mute();
        }
        let menu_in = ui::MenuInput {
            nav: app.ui.repeat(keys.dir),
            activate: keys.activate,
            tab: keys.tab,
            back: false,
            start: keys.start,
        };

        // Simulation, or the drifting field behind the menus.
        let dt = f64::from(get_frame_time()).min(MAX_FRAME_DT);
        match app.screen {
            Screen::Play => {
                if presses.pause && !smoke {
                    audio.set_thrust(0.0, 0.0, false);
                    app.open(Screen::Paused);
                } else {
                    play_steps(
                        &mut app,
                        &presses,
                        &mut controls,
                        &mut audio,
                        smoke,
                        &mut sm,
                    );
                    if app.world.shift_over {
                        app.end_shift(&mut audio);
                    }
                }
            }
            Screen::Paused => {}
            _ => {
                app.world.step_ambient(dt);
                app.fx.update(dt);
                audio.set_thrust(0.0, 0.0, false);
            }
        }

        // Draw the field, then the HUD or the screen's card.
        let dpi = screen_dpi_scale();
        draw::render(draw::Scene {
            look: &look,
            world: &app.world,
            fx: &app.fx,
            stars: &stars,
            vignette: &mut vignette,
            rng: &mut fx_rng,
            device: controls.last_device,
            view: (vw, vh),
            dpi,
            show_tug: app.screen != Screen::Title,
            playing: app.screen == Screen::Play,
        });
        if matches!(app.screen, Screen::Play | Screen::Paused) {
            radar::indicators(&look, &app.world, (vw, vh), dpi);
            radar::radar(&app.world, (vw, vh));
            hud::draw(
                &look,
                &app.world,
                app.career.as_ref(),
                (vw, vh),
                controls.last_device,
            );
        }
        let picked = match app.screen {
            Screen::Play => None,
            Screen::Paused => {
                ui::backdrop((vw, vh));
                let mut menu = menu_in;
                menu.back = presses.pause; // P or Esc resumes
                screens::pause(&look, &mut app.ui, &menu, (vw, vh), dpi, app.armed_menu)
            }
            Screen::Title => {
                ui::backdrop((vw, vh));
                let v = screens::TitleView {
                    saved: app.saved.as_ref(),
                    mode: app.mode,
                    best: app.bests.get(app.mode),
                    tab: &mut app.title_tab,
                    armed_new: app.armed_new,
                };
                screens::title(&look, &mut app.ui, &menu_in, (vw, vh), dpi, v)
            }
            Screen::Report => {
                ui::backdrop((vw, vh));
                let rep = app.report.as_ref().expect("a report");
                screens::report(&look, &mut app.ui, &menu_in, (vw, vh), dpi, rep)
            }
            Screen::Shop => {
                ui::backdrop((vw, vh));
                let v = screens::ShopView {
                    career: app.career.as_ref().expect("a career"),
                    tab: &mut app.shop_tab,
                    msg: &app.shop_msg,
                    t: get_time() as f32,
                };
                screens::shop(&look, &mut app.ui, &menu_in, (vw, vh), dpi, v, &mut fx_rng)
            }
        };
        let picked = if smoke { sm.drive(&mut app) } else { picked };
        if let Some(a) = picked {
            app.act(a, &mut audio);
        }
        if smoke && sm.capture(&app, start) {
            sm.finish(&app, &audio, get_time() - start);
            // The check's save folder was its own; leave nothing behind.
            if let Some(dir) = app.store.folder() {
                let _ = std::fs::remove_dir_all(dir);
            }
            std::process::exit(0);
        }
        if app.quit {
            break;
        }
        next_frame().await;
    }
}

/// Run this frame's fixed simulation steps (frame() in src/js/19-boot.js).
fn play_steps(
    app: &mut App,
    presses: &controls::Presses,
    controls: &mut controls::Controls,
    audio: &mut audio::Audio,
    smoke: bool,
    sm: &mut Smoke,
) {
    // One-shot cable commands land before the next step, as the browser's
    // key handlers run between frames.
    for a in &presses.actions {
        app.world.action(*a);
    }
    if smoke {
        smoke_script(&mut app.world, sm.frame);
    }
    let input = if smoke {
        smoke_input(sm.frame)
    } else {
        controls.sample()
    };
    // Fixed-step; a long stall drops the backlog instead of spiralling.
    let n_max = if smoke { 1 } else { MAX_STEPS_PER_FRAME };
    app.accumulator += if smoke {
        STEP
    } else {
        f64::from(get_frame_time()).min(MAX_FRAME_DT)
    };
    let mut n = 0;
    while app.accumulator >= STEP && n < n_max && !app.world.shift_over {
        app.world.step(&input, STEP);
        app.fx.absorb(&app.world.events);
        audio.tick(STEP);
        for e in &app.world.events {
            match e {
                Event::Sfx(s) => {
                    audio.sfx(*s);
                    sm.sfx += 1;
                }
                Event::Thrust {
                    level,
                    load,
                    sputtering,
                } => audio.set_thrust(*level, *load, *sputtering),
                _ => {}
            }
        }
        app.world.events.clear();
        app.fx.emit_ambient(&app.world);
        app.fx.update(STEP);
        app.accumulator -= STEP;
        n += 1;
        sm.steps += 1;
    }
    if n == MAX_STEPS_PER_FRAME {
        app.accumulator = 0.0;
    }
}

/* ------------------------------ Smoke run ------------------------------ */

/// Frames of scripted flight before the shift is cut short.
const SMOKE_FRAMES: u32 = 150;

/// The unattended check's progress through the screens.
#[derive(Default)]
struct Smoke {
    frame: u32,
    steps: u64,
    sfx: u64,
    /// Frames spent on the current screen.
    on_screen: u32,
    last: Option<Screen>,
    shots: Vec<String>,
    lines: Vec<String>,
    bought: bool,
}

impl Smoke {
    /// The scripted player: fly, pause, end the shift, shop, go to the title.
    fn drive(&mut self, app: &mut App) -> Option<Act> {
        if self.last != Some(app.screen) {
            self.last = Some(app.screen);
            self.on_screen = 0;
        }
        self.on_screen += 1;
        if app.screen == Screen::Play {
            self.frame += 1;
        }
        // Each screen draws for a few frames, is captured on its third
        // (capture runs after drawing), and is left on its fourth.
        let leave = self.on_screen == 4;
        match app.screen {
            Screen::Play if self.frame == SMOKE_FRAMES => {
                app.open(Screen::Paused);
                None
            }
            Screen::Paused if leave => Some(Act::EndShiftNow),
            Screen::Report if leave => {
                if let Some(c) = app.career.as_mut() {
                    // Enough in the wallet to show a purchase, and a
                    // grudge for the truce on the standing tab.
                    c.wallet += 5000.0;
                    c.grudge = 6.0;
                }
                Some(Act::ReportNext)
            }
            Screen::Shop if self.on_screen == 2 && !self.bought => {
                self.bought = true;
                Some(Act::Buy(Part::Thrusters))
            }
            // Then the Parts tab, as LB / RB would switch to it.
            Screen::Shop if leave => {
                app.shop_tab = 1;
                None
            }
            Screen::Shop if self.on_screen == 7 => {
                let c = app.career.as_ref().expect("a career");
                self.lines.push(format!(
                    "  shop: {} wallet {} cr; thrusters mounted: {}",
                    app.shop_msg,
                    c.wallet,
                    c.is_mounted(Part::Thrusters)
                ));
                app.shop_tab = 2;
                Some(Act::Offer(0))
            }
            Screen::Shop if self.on_screen == 8 => Some(Act::Crew(cj_sim::field::Crew::Broker)),
            Screen::Shop if self.on_screen == 11 => {
                let c = app.career.as_ref().expect("a career");
                self.lines.push(format!(
                    "  jobs: {} offers; contract {:?}; crew {:?}; {}",
                    c.offers.len(),
                    c.contract.as_ref().map(|k| k.text()),
                    c.crew.map(|m| m.def().name),
                    app.shop_msg
                ));
                app.shop_tab = 3;
                Some(Act::Truce)
            }
            Screen::Shop if self.on_screen == 14 => {
                let c = app.career.as_ref().expect("a career");
                self.lines.push(format!(
                    "  map: grudge {} rivals standing {}; {}",
                    c.grudge, c.rep.rivals, app.shop_msg
                ));
                Some(Act::ToMenu)
            }
            _ => None,
        }
    }

    /// Save a screenshot of each screen once it has settled; true when the
    /// tour is done.
    fn capture(&mut self, app: &App, start: f64) -> bool {
        let name = match app.screen {
            Screen::Play if self.frame == SMOKE_FRAMES - 1 => "play",
            Screen::Paused if self.on_screen == 3 => "pause",
            Screen::Report if self.on_screen == 3 => "report",
            Screen::Shop if self.on_screen == 3 => "shop",
            Screen::Shop if self.on_screen == 6 => "parts",
            Screen::Shop if self.on_screen == 10 => "jobs",
            Screen::Shop if self.on_screen == 13 => "map",
            Screen::Title if self.on_screen == 3 && self.shots.len() == 7 => "title",
            _ => return false,
        };
        let path = std::env::temp_dir().join(format!("claim-jumpers-m5-{name}.png"));
        // The framebuffer reads bottom row first and carries the alpha of
        // the last blend; flip it and make it opaque.
        let mut img = get_screen_data();
        let row = usize::from(img.width) * 4;
        let flipped: Vec<u8> = img.bytes.chunks(row).rev().flatten().copied().collect();
        img.bytes = flipped;
        for px in img.bytes.chunks_mut(4) {
            px[3] = 255;
        }
        img.export_png(&path.to_string_lossy());
        if name == "play" {
            let w = &app.world;
            let s = &w.ship;
            self.lines.push(format!(
                "  play: {} frames, {} steps in {:.2} s; tug ({:.1}, {:.1}) hull {:.1} shield {:.1}; chain {} ({:.1} t); {} shots, {} jumpers, {} crabs, {} rocks",
                self.frame, self.steps, get_time() - start, s.b.x, s.b.y, s.hp, s.shield,
                w.chain.len(), w.tow_mass(), w.shots_fired, w.jumpers.len(), w.crabs.len(), w.rocks.len()
            ));
            self.lines.push(format!(
                "  opposition: wanted {}, {} law units, {} collectors, {} nets, {} miners, refinery {:?} with {} drones, {} missiles in flight ({} in the rack), {} sentries",
                w.wanted, w.sheriffs.len(), w.collectors.len(), w.nets.len(), w.miners.len(),
                w.refinery.as_ref().map(|r| r.state), w.drones.len(), w.missiles.len(), s.missiles, w.sentries.len()
            ));
            self.lines.push(format!(
                "  field: {} under {}; ion storm {:.1} s; boss {:?}; hauler {:?} hp; sealed holds {}; keycards {}; contract {:?} at {}",
                w.belt.def().name, w.cond.def().name, w.ion_t,
                w.jumpers.iter().find_map(|j| j.boss.as_ref().map(|b| b.name)),
                w.hauler.as_ref().map(|h| h.hp),
                w.wrecks.iter().filter(|x| x.locked).count(),
                w.items.iter().filter(|i| i.kind == ItemType::Keycard).count(),
                w.contract.as_ref().map(|k| k.text()),
                w.contract.as_ref().map_or(0.0, |k| w.contract_progress(k))
            ));
        }
        if name == "report" {
            let r = app.report.as_ref().expect("a report");
            let c = &r.career;
            self.lines.push(format!(
                "  report: earned {} cr, installment {} cr, wallet {} cr, loan {} cr, next shift {}, outcome {:?}",
                r.settlement.earned, r.settlement.due, c.wallet, c.loan, c.shift, r.settlement.outcome
            ));
        }
        self.shots.push(path.display().to_string());
        name == "title"
    }

    fn finish(&self, app: &App, audio: &audio::Audio, secs: f64) {
        println!("M5 SMOKE REPORT");
        println!(
            "  view: {:.0} x {:.0} logical px, dpi scale {:.2}",
            app.world.view.0,
            app.world.view.1,
            screen_dpi_scale()
        );
        for l in &self.lines {
            println!("{l}");
        }
        let saved = app.store.load_career();
        println!(
            "  save: folder {}; career on disk: {}",
            app.store
                .folder()
                .map_or("none".into(), |p| p.display().to_string()),
            saved.map_or("none".to_owned(), |c| format!(
                "shift {}, wallet {} cr, {} parts owned",
                c.shift,
                c.wallet,
                c.owned.len()
            ))
        );
        println!(
            "  title shows: {}",
            app.saved
                .as_ref()
                .map_or("Start career".to_owned(), |c| format!(
                    "Continue · shift {}",
                    c.shift
                ))
        );
        println!(
            "  sound: {} ({} effects requested)",
            audio.error.as_deref().unwrap_or("output stream open"),
            self.sfx
        );
        println!("  total {secs:.2} s");
        for s in &self.shots {
            println!("  screenshot: {s}");
        }
    }
}

/// Smoke script: put a piece of ore beside the tug and hook it, with a
/// rival cutter and a riding crab close by so combat is on screen.
fn smoke_script(w: &mut World, frame: u32) {
    if frame == 1 {
        // Start well clear of the bay, or the bay banks the ore at once.
        w.ship.b.x = BAY_X - 500.0;
        let it = w.new_item(ItemType::Ore, BAY_X - 500.0, BAY_Y + 110.0, 0.0, 0.0);
        let host = it.id;
        w.items.push(it);
        w.ship.invuln = 0.0;
        let crab = cj_sim::entities::make_crab(
            &mut w.rng,
            cj_sim::entities::CrabState::Ride,
            Some(host),
            BAY_X - 500.0,
            BAY_Y + 110.0,
        );
        w.crabs.push(crab);
        let j = w.new_jumper(BAY_X - 1100.0, BAY_Y - 200.0, Some(Variant::Cutter));
        w.jumpers.push(j);
        // Some salvage already banked, so the report has numbers in it.
        w.banked = 1840.0;
        // The opposition on screen: a missile rack and drone bay with three
        // sentries, the refinery close by (its drones wake), a debt
        // collector, a mining crew, and two wanted stars bringing the law.
        let mut mounted = [None; 5];
        mounted[0] = Some(cj_sim::parts::Part::Cannon);
        mounted[1] = Some(cj_sim::parts::Part::Missiles);
        mounted[3] = Some(cj_sim::parts::Part::DroneBay);
        w.mods = cj_sim::mods::compute_mods(&mounted, &cj_sim::parts::Levels::default(), 0);
        for i in 0..3 {
            let s = w.make_sentry(i, 3);
            w.sentries.push(s);
        }
        w.spawn_refinery();
        if let Some(r) = w.refinery.as_mut() {
            r.b.x = BAY_X - 450.0;
            r.b.y = BAY_Y - 650.0;
        }
        w.spawn_collector();
        if let Some(c) = w.collectors.last_mut() {
            c.b.x = BAY_X - 150.0;
            c.b.y = BAY_Y + 450.0;
        }
        w.spawn_miners(2);
        for (i, m) in w.miners.iter_mut().enumerate() {
            m.b.x = BAY_X - 1000.0 + i as f64 * 60.0;
            m.b.y = BAY_Y + 250.0;
        }
        let (x, y) = (w.ship.b.x, w.ship.b.y);
        w.add_wanted(2, x, y);
        w.reinforce_t = Some(0.0);
        // The field systems: a named belt and condition, a contract on the
        // HUD, an ion storm, a boss crew and an escorted hauler close by,
        // and a sealed hold with its keycard.
        w.belt = cj_sim::field::Belt::Kessler;
        w.cond = cj_sim::field::Condition::IonFront;
        w.contract = Some(cj_sim::field::make_contract(
            &mut w.rng,
            cj_sim::field::ContractKind::Bank,
            2,
        ));
        w.start_event(cj_sim::field::Event::Ion);
        w.start_event(cj_sim::field::Event::Boss);
        if let Some(j) = w.jumpers.last_mut() {
            j.b.x = BAY_X - 800.0;
            j.b.y = BAY_Y + 500.0;
        }
        w.start_event(cj_sim::field::Event::Convoy);
        if let Some(h) = w.hauler.as_mut() {
            h.b.x = BAY_X - 1250.0;
            h.b.y = BAY_Y + 80.0;
        }
        w.setup_locked_hold();
        if let Some(wr) = w.wrecks.iter_mut().find(|x| x.locked) {
            wr.b.x = BAY_X - 1500.0;
            wr.b.y = BAY_Y - 450.0;
            wr.b.vx = 0.0;
            wr.b.vy = 0.0;
        }
        if let Some(k) = w.items.iter_mut().find(|i| i.kind == ItemType::Keycard) {
            k.b.x = BAY_X - 700.0;
            k.b.y = BAY_Y - 300.0;
        }
    }
    if frame == 3 {
        w.action(Action::Hook);
    }
}

/// Smoke flight: thrust away from the carrier, then swing the cargo round,
/// firing the cannon throughout.
fn smoke_input(frame: u32) -> Input {
    Input {
        thrust: if frame < 120 { 1.0 } else { 0.0 },
        turn: if (40..100).contains(&frame) { 1.0 } else { 0.0 },
        fire: true,
        ..Input::default()
    }
}
