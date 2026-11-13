//! Claim Jumpers, native Windows build. Fly the tug, tow and fight through five-minute shifts against
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
mod expiry;
mod focus;
mod foes;
mod fx;
mod host;
mod hud;
mod lines;
mod log;
mod look;
mod marge;
mod music;
mod net;
mod online;
mod perf;
mod post;
mod profile;
mod radar;
mod recordings;
mod replay;
mod save;
mod screens;
mod settings;
mod sounds;
mod tug;
mod ui;

use cj_sim::career::{Career, Mode, Outcome, Settlement, MODES};
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
        // A stage build says so, so it can't be mistaken for the real game.
        window_title: if std::env::var_os("CLAIM_JUMPERS_STAGE").is_some() {
            "Claim Jumpers [STAGE]"
        } else {
            "Claim Jumpers"
        }
        .to_owned(),
        window_width: 1280,
        window_height: 720,
        high_dpi: true,
        sample_count: 4,
        window_resizable: true,
        // The title-bar and taskbar icon (tools/make_icon.py draws it).
        icon: Some(miniquad::conf::Icon {
            small: *include_bytes!("../../../assets/icon/icon16.rgba"),
            medium: *include_bytes!("../../../assets/icon/icon32.rgba"),
            big: *include_bytes!("../../../assets/icon/icon64.rgba"),
        }),
        ..Default::default()
    }
}

/// Logical window size (macroquad already divides out the DPI scale).
fn view() -> (f32, f32) {
    (screen_width(), screen_height())
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Screen {
    /// The first screen: single player, host a server, or join one.
    Launch,
    /// Typing the address of a server to join, and seeing how full it is.
    Join,
    /// Online, between shifts or waiting to be let into the next one.
    Lobby,
    Title,
    Play,
    Paused,
    Report,
    Shop,
    Settings,
    Records,
    Board,
    Sounds,
}

/// Files in the save folder beside the career (the browser's other
/// localStorage keys), all in the browser's JSON shapes.
const SETTINGS_FILE: &str = "settings.json";
const PROFILE_FILE: &str = "profile.json";
const BOARD_FILE: &str = "board.json";
const PILOT_FILE: &str = "pilot.txt";

/// How long an achievement toast stays up (seconds).
const TOAST_TIME: f64 = 3.5;
/// How long a save warning stays up (seconds): longer, as it matters more.
const SAVE_WARNING_TIME: f64 = 8.0;
/// Seconds a two-press button stays armed before it resets itself.
const ARM_TIMEOUT: f64 = 3.0;
/// How long the host's "others join with ..." note stays up (seconds).
const HOST_TOAST_TIME: f64 = 20.0;
/// Seconds of hosting alone before the host is told why others may not be
/// getting in.
const ALONE_HINT_AFTER: f64 = 120.0;

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
    /// The store's career generation `saved` was read at.
    saved_gen: u64,
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
    armed_end: bool,
    /// When a two-press button was armed: it disarms by itself after
    /// ARM_TIMEOUT, so a stale arm can't catch a later stray press.
    armed_at: f64,
    accumulator: f64,
    seeded: bool,
    quit: bool,
    /// Marge's radio panel and the shift's highlight recorder.
    radio: marge::RadioPanel,
    recorder: replay::Recorder,
    /// Real seconds left of hit-stop (frozen) and slow motion.
    hitstop_t: f64,
    slow_t: f64,
    voice: marge::Voice,
    music: music::Music,
    settings: settings::Settings,
    /// Settings changed: re-apply palette, motion and key bindings.
    settings_dirty: bool,
    settings_msg: String,
    /// The action waiting for its new key on the settings screen.
    rebinding: Option<settings::Bind>,
    profile: profile::Profile,
    board: Vec<profile::Score>,
    board_tab: usize,
    pilot: String,
    /// The pilot name being typed on the title screen.
    name_edit: Option<String>,
    name_msg: String,
    /// Where Back goes from settings (the title or the pause card).
    menu_from: Screen,
    sound_groups: Vec<(&'static str, Vec<sounds::Item>)>,
    bench: sounds::Bench,
    /// The last achievement earned, and the clock time it fades.
    toast: Option<(String, f64)>,
    /// Messages waiting their turn (text, seconds to show), so a second
    /// message (an achievement while hosting, say) doesn't wipe the first.
    toasts: std::collections::VecDeque<(String, f64)>,
    /// A connection to a server while playing online (see `net`).
    online: Option<net::Online>,
    /// The server this game is hosting, if any (see `host`).
    hosted: Option<host::Hosted>,
    /// When hosting began, and whether the "nobody has joined" hint has
    /// been shown for it.
    hosted_at: f64,
    alone_hinted: bool,
    /// The address typed on the Join screen.
    join_addr: String,
    /// This player said they are ready in the current break.
    ready_sent: bool,
    /// The public chat seen on this server, and the line being typed.
    chat: online::Chat,
    /// How many players a hosted server is for (it also sizes the field).
    host_limit: usize,
    /// The difficulty mode of careers on a server this player hosts.
    host_mode: Mode,
    /// Whether a server this player hosts allows PvP (off by default).
    host_pvp: bool,
    /// Online: this player's career on the server, as of the last report
    /// (never the single-player career, which stays in `career`).
    online_career: Option<Career>,
    /// A daily challenge in progress: its day and its fixed loadout.
    daily: Option<(String, Career)>,
    /// The shift was quit from the pause card: its report's Next leads to
    /// the title rather than the shop.
    quit_to_title: bool,
    /// A Join-screen address being looked up: what was typed, and where the
    /// worker thread sends the answer.
    lookup: Option<(String, std::sync::mpsc::Receiver<Result<String, String>>)>,
}

/// True when the career goes on after this report (to the shop).
fn continues(rep: Option<&Report>) -> bool {
    matches!(
        rep.map(|r| &r.settlement.outcome),
        Some(Outcome::Continue | Outcome::TradedUp { .. })
    )
}

/// At most this many messages wait behind the one showing; past it the
/// oldest waiting one is dropped.
const TOAST_QUEUE: usize = 3;

impl App {
    /// Queues a toast to show for `secs` once those before it are done.
    fn say(&mut self, text: String, secs: f64) {
        if self.toasts.len() >= TOAST_QUEUE {
            self.toasts.pop_front();
        }
        self.toasts.push_back((text, secs));
    }

    /// Moves to the next waiting toast once the current one has had its time.
    fn next_toast(&mut self) {
        let done = self
            .toast
            .as_ref()
            .is_none_or(|(_, until)| get_time() > *until);
        if done {
            self.toast = self
                .toasts
                .pop_front()
                .map(|(text, secs)| (text, get_time() + secs));
        }
    }

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
        self.armed_end = false;
        if s == Screen::Title {
            // Only re-read when the career was saved since: a damaged file
            // would otherwise be backed up again on every visit.
            if self.saved_gen != self.store.career_gen() {
                self.saved = self.store.load_career();
                self.saved_gen = self.store.career_gen();
            }
        }
        // Online the server rolls the offers and conditions. Offline the
        // roll is saved at once: otherwise going back to the title and
        // pressing Continue reloads the career without it and rolls again.
        if s == Screen::Shop && self.online.is_none() {
            let mut rng = self.rng();
            if let Some(c) = self.career.as_mut() {
                c.ensure_offers(&mut rng);
                c.ensure_conditions(&mut rng);
                self.store.save_career(c);
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
        // Training runs on a career's first shift until it has been done once.
        if self.store.training_pending(c.shift) {
            self.world.start_tutorial();
        }
        if self.settings.coop != settings::Coop::Off {
            self.world.spawn_partner();
        }
        self.begin_play(audio);
    }

    /// Connects to `server` ("host:port"). With `look_first` it only asks
    /// how full the server is, for the Join screen ([`App::join_online`]
    /// joins); otherwise the online shift starts at once.
    fn start_online(
        &mut self,
        server: &str,
        look_first: bool,
        audio: &mut audio::Audio,
    ) -> Result<(), String> {
        let id = net::identity(&self.store);
        let view = (self.world.view.0 as u16, self.world.view.1 as u16);
        let on = net::Online::connect(server, id, &self.pilot, view, look_first)?;
        self.online = Some(on);
        if !look_first {
            self.join_online(audio);
        }
        Ok(())
    }

    /// Joins the server connected to and starts flying (or waiting in the
    /// lobby) once it answers.
    fn join_online(&mut self, audio: &mut audio::Audio) {
        if let Some(on) = self.online.as_mut() {
            on.join();
        }
        self.ready_sent = false;
        self.world = World::empty(Rng::Constant(0.5), self.world.view);
        self.begin_play(audio);
    }

    /// Connects to a Join-screen address once its lookup has answered.
    fn poll_lookup(&mut self, audio: &mut audio::Audio) {
        let Some((_, rx)) = self.lookup.as_ref() else {
            return;
        };
        let answer = match rx.try_recv() {
            Ok(a) => a,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Err("The address lookup stopped. Try again.".to_owned())
            }
        };
        self.lookup = None;
        if let Err(e) = answer.and_then(|addr| self.start_online(&addr, true, audio)) {
            self.say(e, SAVE_WARNING_TIME);
        }
    }

    /// The Join screen's line about the server being looked up, and whether
    /// it can be joined.
    fn join_status(&self) -> (Option<(String, u32)>, bool, bool) {
        if let Some((server, _)) = self.lookup.as_ref() {
            return (
                Some((format!("Looking up {server}..."), look::col::MUTED)),
                false,
                true,
            );
        }
        let Some(on) = self.online.as_ref().filter(|o| o.looking()) else {
            return (None, false, false);
        };
        let mine = env!("CARGO_PKG_VERSION");
        match &on.session.info {
            None => (
                Some(("Asking the server...".to_owned(), look::col::MUTED)),
                false,
                true,
            ),
            Some(i) if i.version != mine => (
                Some((
                    format!(
                        "That server runs Claim Jumpers {} and you have {mine}.",
                        i.version
                    ),
                    look::col::DANGER,
                )),
                false,
                true,
            ),
            Some(i) if i.full => (
                Some((
                    format!(
                        "Full: {} of {} pilots. Try again later.",
                        i.connected, i.max
                    ),
                    look::col::DANGER,
                )),
                false,
                true,
            ),
            Some(i) => (
                Some((
                    format!(
                        "{} of {} pilots connected. There's room for you.{}",
                        i.connected,
                        i.max,
                        // So nobody joins a PvP server by surprise.
                        if i.pvp { " PvP is on." } else { "" }
                    ),
                    look::col::ORE,
                )),
                true,
                true,
            ),
        }
    }

    /// Hosts a server on this machine (port 47820) and joins it, telling the
    /// player the address others should type.
    fn host(&mut self, audio: &mut audio::Audio) {
        // Hosted careers live in their own folder, never beside career.json.
        let careers = self
            .store
            .folder()
            .map(|d| d.join("servers").join("hosted"));
        let hosted = match host::Hosted::start(
            env!("CARGO_PKG_VERSION"),
            self.host_limit,
            self.host_mode,
            self.host_pvp,
            careers,
        ) {
            Ok(h) => h,
            Err(e) => {
                self.say(e, SAVE_WARNING_TIME);
                return;
            }
        };
        self.hosted = Some(hosted);
        self.hosted_at = get_time();
        self.alone_hinted = false;
        let own = format!("localhost:{}", cj_net::DEFAULT_PORT);
        match self.start_online(&own, false, audio) {
            Ok(()) => {
                let tell = match host::lan_address() {
                    Some(ip) => format!(
                        "Hosting. Others join with {ip}. If Windows asks, allow Claim Jumpers on private networks."
                    ),
                    None => "Hosting on port 47820.".to_owned(),
                };
                self.say(tell, HOST_TOAST_TIME);
            }
            Err(e) => {
                self.hosted = None;
                self.say(e, SAVE_WARNING_TIME);
            }
        }
    }

    /// Ends an online session for `why` (shown as a toast), stops a hosted
    /// server, and goes back to the launch screen over a fresh field (or
    /// stays on the Join screen if the server was only being looked up).
    fn leave_online(&mut self, why: String) {
        let back = if self.screen == Screen::Join {
            Screen::Join
        } else {
            Screen::Launch
        };
        if let Some(mut on) = self.online.take() {
            on.leave();
        }
        // Dropping the hosted server stops it and disconnects everyone.
        self.hosted = None;
        self.online_career = None;
        self.chat.clear();
        self.world = World::new_shift(self.rng(), self.world.view);
        self.say(why, SAVE_WARNING_TIME);
        self.open(back);
    }

    /// A host still alone after a while is told the usual reasons others
    /// can't get in. A firewall that drops their packets can't be seen from
    /// this side, so a quiet server is the only sign there is.
    fn hint_if_alone(&mut self) {
        let Some(on) = self.online.as_ref().filter(|_| self.hosted.is_some()) else {
            return;
        };
        let others = on
            .session
            .lobby
            .as_ref()
            .map_or(0, |l| l.players.len().saturating_sub(1));
        if self.alone_hinted || others > 0 || get_time() - self.hosted_at < ALONE_HINT_AFTER {
            return;
        }
        self.alone_hinted = true;
        let at = host::lan_address().map_or_else(String::new, |ip| format!(" with {ip}"));
        self.say(format!("No one has joined yet. Check they join{at}, and that Windows Firewall allows Claim Jumpers on private networks."), HOST_TOAST_TIME);
    }

    /// Online screen changes the server drives: to the lobby when a shift
    /// ends (or this player is told to wait), to the shift's report once
    /// the server has settled it, and back into a shift when the next one
    /// starts.
    fn follow_server(&mut self, audio: &mut audio::Audio) {
        // A shop answer updates this player's copy of their career.
        let reply = self.online.as_mut().and_then(|o| o.session.shop.take());
        if let Some((ok, msg, career)) = reply {
            if ok && !msg.is_empty() {
                audio.sfx(Sfx::Hook);
            }
            if !msg.is_empty() {
                self.shop_msg = msg;
            }
            self.online_career = Some(career);
        }
        let settled = self.online.as_mut().and_then(|o| o.session.settled.take());
        if let Some((st, career)) = settled {
            // Online careers have no local best: the report shows none.
            let rep = screens::Report::new(st, &self.world, &career, 0.0, false);
            self.report = Some(rep);
            self.online_career = Some(career);
            audio.set_thrust(0.0, 0.0, false);
            self.open(Screen::Report);
        }
        let Some(on) = self.online.as_ref().filter(|o| !o.looking()) else {
            return;
        };
        let between = on.session.waiting || on.session.in_break();
        let flying = on.session.pilot.is_some() && !between;
        match self.screen {
            Screen::Play | Screen::Paused if between => {
                audio.set_thrust(0.0, 0.0, false);
                self.ready_sent = false;
                self.open(Screen::Lobby);
            }
            Screen::Lobby | Screen::Report | Screen::Shop if flying => {
                self.ready_sent = false;
                self.world = World::empty(Rng::Constant(0.5), self.world.view);
                self.begin_play(audio);
            }
            _ => {}
        }
    }

    /// Everything a shift resets once its world is ready, career or daily.
    fn begin_play(&mut self, audio: &mut audio::Audio) {
        self.fx = fx::Fx::new(self.rng());
        self.quit_to_title = false;
        self.accumulator = 0.0;
        self.radio.reset();
        self.recorder = replay::Recorder::default();
        self.hitstop_t = 0.0;
        self.slow_t = 0.0;
        audio.set_thrust(0.0, 0.0, false);
        self.open(Screen::Play);
    }

    /// Skip the guided training and remember it, as the browser's
    /// endTutorial does. The step loop can't record this: a skip happens
    /// between steps, so the loop never sees training end.
    fn skip_training(&mut self) {
        if self.world.tut.is_some() {
            self.world.end_tutorial(true);
            self.store.save_training_done();
        }
    }

    /// beginDaily: today's field for everyone, from a seed made of the
    /// date. Only world generation is seeded; play then runs on ordinary
    /// randomness. The career is not touched.
    fn start_daily(&mut self, audio: &mut audio::Audio) {
        let day = daily_day();
        let (mut w, c) = daily_world(&day, self.world.view);
        w.rng = self.rng();
        if self.settings.coop != settings::Coop::Off {
            w.spawn_partner();
        }
        self.world = w;
        self.daily = Some((day, c));
        self.begin_play(audio);
    }

    /// endDaily: score the run on the daily board and show its report.
    fn end_daily(&mut self, audio: &mut audio::Audio) {
        let Some((day, c)) = self.daily.take() else {
            return;
        };
        let earned = cj_sim::js::round(self.world.me().banked);
        let key = format!("daily-{day}");
        let prev = profile::board_for(&self.board, &key)
            .first()
            .map_or(0.0, |s| s.score);
        let is_best = earned > prev;
        if earned > 0.0 {
            profile::add_score(
                &mut self.board,
                profile::Score {
                    name: self.pilot.clone(),
                    mode: key,
                    score: earned,
                    shift: 1,
                    at: now_ms(),
                },
            );
            self.store
                .write(BOARD_FILE, &profile::board_to_json(&self.board));
        }
        let st = Settlement {
            earned,
            due: 0.0,
            contract: None,
            wage: 0.0,
            insurance: 0.0,
            chain_note: String::new(),
            rep_delta: [0.0; 3],
            outcome: Outcome::Continue,
        };
        let mut rep = Report::new(st, &self.world, &c, prev.max(earned), is_best);
        rep.daily = Some(day);
        rep.highlight = self.recorder.finish();
        self.report = Some(rep);
        audio.set_thrust(0.0, 0.0, false);
        self.open(Screen::Report);
    }

    /// endShift: settle the career and show the report.
    fn end_shift(&mut self, audio: &mut audio::Audio) {
        if self.daily.is_some() {
            self.end_daily(audio);
            return;
        }
        let Some(c) = self.career.as_mut() else {
            return;
        };
        let mode = c.mode;
        let flown = c.shift;
        let st: Settlement = c.settle(&self.world, 0);
        let best = self.bests.get(mode).unwrap_or(0.0);
        let is_best = st.earned > best;
        if is_best {
            self.bests.set(mode, st.earned);
            self.store.save_bests(&self.bests);
        }
        match st.outcome {
            Outcome::Continue | Outcome::TradedUp { .. } => {
                // The career as it stood before this shift goes into the
                // rotating backups first.
                self.store.backup_career();
                self.store.save_career(c);
            }
            // A repossession or a paid-off fleet ends the career.
            Outcome::Repossessed { .. } | Outcome::FleetPaid => self.store.clear_career(),
        }
        let best = self.bests.get(mode).unwrap_or(0.0);
        let mut rep = Report::new(st, &self.world, c, best, is_best);
        rep.highlight = self.recorder.finish();
        self.record_shift(&rep.settlement, mode.def().key, flown, audio);
        self.report = Some(rep);
        audio.set_thrust(0.0, 0.0, false);
        self.open(Screen::Report);
    }

    /// The profile and the local board after a shift (endShift's
    /// Profile.bump / unlock calls and submitScore).
    fn record_shift(&mut self, st: &Settlement, mode: &str, flown: i32, audio: &mut audio::Audio) {
        let w = &self.world;
        let p = &mut self.profile;
        let earned = st.earned;
        let met = st.contract.as_ref().is_some_and(|k| k.1);
        if met {
            p.bump("contracts", 1.0);
        }
        p.bump("shifts", 1.0);
        p.bump("banked", earned);
        p.bump("deaths", f64::from(w.me().stats.deaths));
        p.best(earned);
        p.bump("kills", f64::from(w.me().stats.jumpers));
        p.bump("bosses", f64::from(w.me().kills.boss));
        for (i, n) in profile::kill_counts(&w.me().kills).into_iter().enumerate() {
            p.kills[i] += n;
        }
        let mut earned_ids = Vec::new();
        if met {
            earned_ids.push("contract");
        }
        if w.me().stats.deaths == 0 && earned >= 3000.0 {
            earned_ids.push("clean");
        }
        if w.belt == cj_sim::field::Belt::Frontier && !w.me().impounded {
            earned_ids.push("frontier");
        }
        match st.outcome {
            Outcome::TradedUp { .. } => earned_ids.push("tradeUp"),
            Outcome::FleetPaid => earned_ids.push("fleet"),
            _ => {}
        }
        for id in earned_ids {
            self.unlock(id, audio);
        }
        if earned > 0.0 {
            profile::add_score(
                &mut self.board,
                profile::Score {
                    name: self.pilot.clone(),
                    mode: mode.to_owned(),
                    score: cj_sim::js::round(earned),
                    shift: i64::from(flown),
                    at: now_ms(),
                },
            );
            self.store
                .write(BOARD_FILE, &profile::board_to_json(&self.board));
        }
        self.store.write(PROFILE_FILE, &self.profile.to_json());
    }

    /// Profile.unlock: once, with a toast and the jackpot chime.
    fn unlock(&mut self, id: &str, audio: &mut audio::Audio) {
        let Some(title) = self.profile.unlock(id, now_ms()) else {
            return;
        };
        let how = profile::ACHIEVEMENTS
            .iter()
            .find(|a| a.0 == id)
            .map_or("", |a| a.2);
        self.say(format!("Achievement: {title} · {how}"), TOAST_TIME);
        audio.sfx(Sfx::Jackpot);
        self.store.write(PROFILE_FILE, &self.profile.to_json());
    }

    fn save_settings(&mut self) {
        self.store.write(SETTINGS_FILE, &self.settings.to_json());
        self.settings_dirty = true;
    }

    /// The loan was paid off from the shop: the report screen shows the
    /// trade-up (or the won career), as showPaidOff does.
    fn paid_off_in_shop(&mut self, outcome: Outcome, audio: &mut audio::Audio) {
        self.unlock(
            if outcome == Outcome::FleetPaid {
                "fleet"
            } else {
                "tradeUp"
            },
            audio,
        );
        let c = self.career.as_ref().expect("a career");
        if outcome == Outcome::FleetPaid {
            self.store.clear_career();
        } else {
            self.store.save_career(c);
        }
        // No shift behind this report: blankShiftReport in the browser.
        let mut rep = Report::new(
            Settlement {
                earned: 0.0,
                due: 0.0,
                contract: None,
                wage: 0.0,
                insurance: 0.0,
                chain_note: String::new(),
                rep_delta: [0.0; 3],
                outcome,
            },
            &self.world,
            c,
            self.bests.get(c.mode).unwrap_or(0.0),
            false,
        );
        rep.no_shift = true;
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
        // Online the shop belongs to the server: its buttons become requests
        // (and the belt a vote), answered with the career as it now stands.
        if let Some(req) = self.online.as_ref().and_then(|_| shop_request(a)) {
            if let Some(on) = self.online.as_mut() {
                let msg = on.session.shop_req(req);
                on.send(msg);
            }
            return;
        }
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
                    self.armed_at = get_time();
                    return;
                }
                self.store.clear_career();
                let c = Career::new(self.mode);
                self.store.save_career(&c);
                self.career = Some(c);
                self.start_shift(audio);
            }
            Act::Quit => self.quit = true,
            Act::OpenSettings => {
                self.menu_from = self.screen;
                self.settings_msg.clear();
                self.rebinding = None;
                self.open(Screen::Settings);
            }
            Act::OpenRecords => self.open(Screen::Records),
            Act::OpenBoard => {
                self.board_tab = cj_sim::career::MODES
                    .iter()
                    .position(|m| *m == self.mode)
                    .unwrap_or(1);
                self.open(Screen::Board);
            }
            Act::OpenSounds => self.open(Screen::Sounds),
            Act::SinglePlayer => self.open(Screen::Title),
            Act::Host => self.host(audio),
            Act::CycleLimit => {
                const LIMITS: [usize; 6] = [2, 4, 8, 12, 16, 20];
                let at = LIMITS
                    .iter()
                    .position(|&n| n == self.host_limit)
                    .unwrap_or(0);
                self.host_limit = LIMITS[(at + 1) % LIMITS.len()];
            }
            Act::CycleHostMode => {
                let at = MODES.iter().position(|&m| m == self.host_mode).unwrap_or(0);
                self.host_mode = MODES[(at + 1) % MODES.len()];
            }
            Act::ToggleHostPvp => self.host_pvp = !self.host_pvp,
            Act::OpenJoin => {
                // Keys typed elsewhere mustn't land in the address.
                while get_char_pressed().is_some() {}
                self.open(Screen::Join);
            }
            Act::Join => {
                if self.online.is_some() {
                    if self.join_status().1 {
                        let typed = self.join_addr.trim().to_owned();
                        self.settings.remember_server(&typed);
                        self.save_settings();
                        self.join_online(audio);
                    }
                    return;
                }
                // The name lookup runs off the frame loop; `poll_lookup`
                // connects once it answers.
                let server = net::server_addr(&self.join_addr);
                self.lookup = Some((server.clone(), net::resolve(server)));
            }
            Act::PickServer(i) => {
                if let Some(server) = self.settings.recent_servers.get(i) {
                    self.join_addr = server.clone();
                    // A different address: stop looking at the old server.
                    self.lookup = None;
                    if let Some(mut on) = self.online.take() {
                        on.leave();
                    }
                }
            }
            Act::Back if self.screen == Screen::Join => {
                self.lookup = None;
                if let Some(mut on) = self.online.take() {
                    on.leave();
                }
                self.open(Screen::Launch);
            }
            Act::Ready => {
                if let Some(on) = self.online.as_mut() {
                    let msg = on.session.ready();
                    on.send(msg);
                    self.ready_sent = true;
                }
            }
            Act::LeaveServer => self.leave_online("You left the server.".to_owned()),
            Act::OpenChat => {
                // Keys typed elsewhere mustn't land in the line.
                while get_char_pressed().is_some() {}
                self.chat.typing = Some(String::new());
            }
            Act::Back => {
                if self.screen == Screen::Sounds {
                    self.bench.stop(audio, &mut self.voice);
                }
                self.rebinding = None;
                let to = if self.screen == Screen::Settings {
                    self.menu_from
                } else {
                    Screen::Title
                };
                self.open(to);
            }
            Act::ReplayTraining => {
                self.store.save_training_replay();
                audio.sfx(Sfx::Hook);
            }
            Act::EditName => match self.name_edit.take() {
                Some(typed) => {
                    let name = profile::clean_name(&typed);
                    if name.is_empty() {
                        self.name_msg = "Use at least one letter or number.".to_owned();
                    } else {
                        self.store.write(PILOT_FILE, &name);
                        self.name_msg = format!("Saved. You fly as {name}.");
                        self.pilot = name;
                    }
                }
                None => {
                    self.name_edit = Some(self.pilot.clone());
                    self.name_msg.clear();
                }
            },
            Act::ToggleVoice => {
                self.voice.toggle();
                self.settings.voice = self.voice.enabled;
                self.save_settings();
            }
            Act::SetMusic => {
                self.settings.music = !self.settings.music;
                self.save_settings();
            }
            Act::SetMotion => {
                self.settings.reduce_motion = !self.settings.reduce_motion;
                self.save_settings();
            }
            Act::SetFullscreen => {
                self.settings.fullscreen = !self.settings.fullscreen;
                self.save_settings();
            }
            Act::SetMono => {
                self.settings.mono = !self.settings.mono;
                self.save_settings();
            }
            Act::SetBackgroundSound => {
                self.settings.background_sound = !self.settings.background_sound;
                self.save_settings();
            }
            Act::SetPalette => {
                self.settings.colorblind = !self.settings.colorblind;
                self.save_settings();
            }
            Act::SetEffects => {
                self.settings.effects = !self.settings.effects;
                self.save_settings();
            }
            Act::SetSprites => {
                self.settings.sprites = !self.settings.sprites;
                self.save_settings();
            }
            Act::CycleHud => {
                self.settings.hud_pct = self.settings.next_hud();
                self.save_settings();
            }
            Act::CycleVolume(k) => {
                self.settings.volume[k] = self.settings.next_volume(k);
                self.save_settings();
            }
            Act::CycleCoop => {
                self.settings.coop = self.settings.coop.next();
                self.save_settings();
            }
            Act::Rebind(b) => {
                self.rebinding = Some(b);
                self.settings_msg = format!("Press the new key for {}.", settings::def(b).2);
            }
            Act::ResetKeys => {
                self.settings.keys.clear();
                self.settings_msg = "Controls reset.".to_owned();
                self.save_settings();
            }
            Act::BoardTab(i) => self.board_tab = i,
            Act::PlaySound(i) => {
                if audio.is_muted() {
                    audio.toggle_mute();
                }
                let items: Vec<&sounds::Item> =
                    self.sound_groups.iter().flat_map(|g| &g.1).collect();
                self.bench.play(&items, i, get_time(), 0.0);
            }
            Act::PlayAll => {
                if audio.is_muted() {
                    audio.toggle_mute();
                }
                self.bench.stop(audio, &mut self.voice);
                let items: Vec<&sounds::Item> =
                    self.sound_groups.iter().flat_map(|g| &g.1).collect();
                self.bench.play_all(&items, get_time());
            }
            Act::StopSounds => self.bench.stop(audio, &mut self.voice),
            Act::Paint(i) => {
                self.profile.paint = profile::PAINTS[i].0.to_owned();
                self.store.write(PROFILE_FILE, &self.profile.to_json());
                audio.sfx(Sfx::Hook);
            }
            Act::Daily => self.start_daily(audio),
            Act::Resume => self.open(Screen::Play),
            // Online, leaving is just leaving: the shared shift goes on, and
            // there's no local career to settle. It takes two presses, as
            // offline, because a host's leave stops the server for everyone.
            Act::EndShiftNow | Act::PauseToMenu if self.online.is_some() => {
                if !self.armed_menu {
                    self.armed_menu = true;
                    self.armed_at = get_time();
                    return;
                }
                self.leave_online("You left the server.".to_owned());
            }
            // Settling is final, so it takes two presses like Main menu: a
            // pad's A on the focused button is an easy slip.
            Act::EndShiftNow => {
                if !self.armed_end {
                    self.armed_end = true;
                    self.armed_menu = false;
                    self.armed_at = get_time();
                    return;
                }
                self.end_shift(audio);
            }
            Act::PauseToMenu => {
                // Quitting mid-shift must not dodge the installment: it ends
                // the shift (two presses). A repossession or payoff stays on
                // the report so the player sees it.
                if !self.armed_menu {
                    self.armed_menu = true;
                    self.armed_end = false;
                    self.armed_at = get_time();
                    return;
                }
                // The report is shown so the player sees what quitting cost
                // (installment, contract, wage); its Next goes to the title.
                self.end_shift(audio);
                self.quit_to_title = true;
            }
            // Online the report leads back to the lobby: the server has
            // already settled the career, and the break goes on there.
            Act::ReportNext if self.online.is_some() => {
                self.shop_msg.clear();
                let next = if self.online_career.is_some() {
                    Screen::Shop
                } else {
                    Screen::Lobby
                };
                self.open(next);
            }
            Act::ReportNext => {
                let quit = std::mem::take(&mut self.quit_to_title);
                if self.report.as_ref().is_some_and(|r| r.daily.is_some())
                    || (quit && continues(self.report.as_ref()))
                {
                    self.go_to_title();
                } else if continues(self.report.as_ref()) {
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
            // Online, leaving the shop or the report goes back to the lobby,
            // and starting the next shift means being ready for it.
            Act::ToMenu if self.online.is_some() => self.open(Screen::Lobby),
            Act::StartShift if self.online.is_some() => {
                if let Some(on) = self.online.as_mut() {
                    let msg = on.session.ready();
                    on.send(msg);
                }
                self.ready_sent = true;
                self.open(Screen::Lobby);
            }
            Act::OpenShop => {
                self.shop_msg.clear();
                self.open(Screen::Shop);
            }
            Act::ToMenu => self.go_to_title(),
            Act::StartShift => self.start_shift(audio),
            // Listed one by one (not `_`) so a new Act must be placed here
            // or above on purpose.
            Act::Pay(_)
            | Act::Buy(_)
            | Act::Mount(_)
            | Act::Unmount(_)
            | Act::BuySentry
            | Act::Level(_)
            | Act::Offer(_)
            | Act::Crew(_)
            | Act::Belt(_)
            | Act::Truce
            | Act::Insure
            | Act::SellCore
            | Act::ReprogramCore => {
                if let Some(req) = shop_request(a) {
                    self.shop_locally(req, audio);
                }
            }
        }
    }

    /// A shop button in single player: the same rules the server applies
    /// online (`ShopReq::apply`), then the sound, the save and the shop's
    /// sentence. A payment that clears the loan opens the trade-up (or
    /// won-career) report instead.
    fn shop_locally(&mut self, req: cj_net::ShopReq, audio: &mut audio::Audio) {
        use cj_net::ShopReq as R;
        let c = self.career.as_mut().expect("a career");
        match req.apply(c) {
            Ok((_, Some(outcome))) => {
                audio.sfx(Sfx::Bank);
                self.paid_off_in_shop(outcome, audio);
            }
            Ok((msg, None)) => {
                // Spending money rings the till; handling gear clanks.
                audio.sfx(match req {
                    R::Mount(_) | R::Offer(_) | R::Crew(_) | R::Vote(_) => Sfx::Hook,
                    R::Unmount(_) => Sfx::Drop,
                    _ => Sfx::Bank,
                });
                self.store.save_career(c);
                self.shop_msg = msg;
            }
            Err(msg) => self.shop_msg = msg,
        }
    }
}

/// The shop request a shop button stands for, if it is one: sent to the
/// server online, applied to the local career in single player.
fn shop_request(a: Act) -> Option<cj_net::ShopReq> {
    use cj_net::ShopReq as R;
    Some(match a {
        Act::Pay(limit) => R::Pay {
            all: limit.is_infinite(),
        },
        Act::Buy(p) => R::Buy(p),
        Act::Mount(p) => R::Mount(p),
        Act::Unmount(p) => R::Unmount(p),
        Act::Level(s) => R::Level(s),
        Act::BuySentry => R::Sentry,
        Act::Offer(i) => R::Offer(u8::try_from(i).ok()?),
        Act::Crew(c) => R::Crew(c),
        Act::Belt(b) => R::Vote(b),
        Act::Truce => R::Truce,
        Act::Insure => R::Insure,
        Act::SellCore => R::SellCore,
        Act::ReprogramCore => R::ReprogramCore,
        _ => return None,
    })
}

/// Shows the expired screen until the player quits (the Quit button, Enter
/// or Escape, or closing the window). Keyboard and mouse only: the pad
/// reader isn't started, and closing the window always works.
async fn expired_screen(look: &look::Look) {
    let mut u = ui::Ui::new();
    loop {
        let menu = ui::MenuInput {
            activate: is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter),
            back: is_key_pressed(KeyCode::Escape),
            ..Default::default()
        };
        let v = view();
        clear_background(BLACK);
        ui::splash(look, v, screen_dpi_scale());
        if screens::expired(look, &mut u, &menu, v, screen_dpi_scale()) || menu.back {
            return;
        }
        next_frame().await;
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
    // One frame of the splash first: the date check below can wait 1.5 s
    // for network time, and a blank window that long looks hung.
    clear_background(BLACK);
    ui::splash(&look, view(), screen_dpi_scale());
    next_frame().await;
    // Past the play-test cut-off nothing else starts: no sound, no saves
    // read or written, no server. The date comes from network time when
    // it answers (up to 1.5 s offline), otherwise the PC's clock.
    if expiry::expired(expiry::now()) {
        if smoke {
            // The check fails, so a build past its date can't pass for a
            // working release.
            eprintln!(
                "smoke: FAIL, this build stopped working on {}",
                expiry::CUT_OFF_TEXT
            );
            std::process::exit(1);
        }
        expired_screen(&look).await;
        return;
    }
    // The save folder first, so the log and the crash hook cover the rest
    // of startup (sound included).
    let store = save::Store::open();
    let crash_note = log::init(store.folder());
    let mut audio = audio::Audio::start();
    if let Some(e) = &audio.error {
        log::line(&format!("sound is off: {e}"));
    }
    let mut controls = controls::Controls::new();
    let mut fx_rng = seeded(7);
    let stars = draw::Stars::new(&mut fx_rng);
    let mut vignette = draw::Vignette::new();
    // Shaders compile here, after the window exists.
    let mut post = post::Post::new();
    let mut perf = perf::Perf::new();
    let (vw, vh) = view();
    let settings = settings::Settings::from_json(store.read_json(SETTINGS_FILE).as_deref());
    let profile = profile::Profile::from_json(store.read_json(PROFILE_FILE).as_deref());
    let board = profile::board_from_json(store.read_json(BOARD_FILE).as_deref());
    // loadPlayerName: a cleaned saved name, or a fresh Tugger-NNNN.
    let pilot = match store.read(PILOT_FILE).map(|n| profile::clean_name(&n)) {
        Some(n) if !n.is_empty() => n,
        _ => {
            let n = format!("Tugger-{}", fx_rng.int(1000, 9999));
            store.write(PILOT_FILE, &n);
            n
        }
    };
    let mut app = App {
        bests: store.load_bests(),
        mode: store.load_mode(),
        saved: store.load_career(),
        saved_gen: store.career_gen(),
        store,
        career: None,
        world: World::new_shift(seeded(7), (f64::from(vw), f64::from(vh))),
        fx: fx::Fx::new(seeded(7)),
        // A check run tours the single-player screens from the title.
        screen: if smoke { Screen::Title } else { Screen::Launch },
        ui: ui::Ui::new(),
        report: None,
        title_tab: 0,
        shop_tab: 0,
        shop_msg: String::new(),
        armed_new: false,
        armed_menu: false,
        armed_end: false,
        armed_at: 0.0,
        accumulator: 0.0,
        seeded: smoke,
        quit: false,
        radio: marge::RadioPanel::default(),
        recorder: replay::Recorder::default(),
        hitstop_t: 0.0,
        slow_t: 0.0,
        // A check run keeps quiet: no speech, and the voice is only probed.
        voice: marge::Voice::new(settings.voice && !smoke),
        music: music::Music::new(settings.music),
        settings,
        settings_dirty: true,
        settings_msg: String::new(),
        rebinding: None,
        profile,
        board,
        board_tab: 1,
        pilot,
        name_edit: None,
        name_msg: String::new(),
        menu_from: Screen::Title,
        sound_groups: sounds::groups(),
        bench: sounds::Bench::default(),
        toast: None,
        toasts: std::collections::VecDeque::new(),
        online: None,
        hosted: None,
        join_addr: String::new(),
        ready_sent: false,
        hosted_at: 0.0,
        alone_hinted: false,
        chat: online::Chat::default(),
        host_limit: 4,
        host_mode: Mode::Standard,
        host_pvp: false,
        online_career: None,
        daily: None,
        quit_to_title: false,
        lookup: None,
    };
    if let Some(note) = crash_note {
        app.say(note, SAVE_WARNING_TIME);
    }
    let mut sm = Smoke::default();
    if smoke {
        app.act(Act::StartOrContinue, &mut audio);
    }
    // `--join HOST:PORT` joins that server at once, skipping the Join
    // screen (handy for testing).
    let join = std::env::args().skip_while(|a| a != "--join").nth(1);
    if let Some(server) = join.filter(|_| !smoke) {
        let (vw, vh) = view();
        app.world.view = (f64::from(vw), f64::from(vh));
        if let Err(e) = app.start_online(&server, false, &mut audio) {
            app.say(e, SAVE_WARNING_TIME);
        }
    }
    let start = get_time();
    let mut focus = focus::Focus::new();
    // What the window is now, so the setting is applied only on a change.
    let mut fullscreen_now = false;
    // F3's smoothed CPU split of a frame, in ms.
    let (mut sim_ms, mut draw_ms) = (0.0f32, 0.0f32);

    loop {
        let pad_lost = controls.poll();
        // Leaving the window (or a pad dropping out) pauses a single-player
        // shift below; online the shared shift can't wait.
        let focus_change = focus.update();
        if focus_change != focus::Change::None {
            audio.set_away(focus.away && !app.settings.background_sound);
        }
        let step_away = (focus_change == focus::Change::Lost || pad_lost) && !smoke;
        if get_time() - app.armed_at > ARM_TIMEOUT {
            app.armed_new = false;
            app.armed_menu = false;
            app.armed_end = false;
        }
        if is_key_pressed(KeyCode::F11) {
            app.act(Act::SetFullscreen, &mut audio);
        }
        audio.recover(get_time());
        let (vw, vh) = view();
        app.world.view = (f64::from(vw), f64::from(vh));
        // Both readers run every frame so their button edges stay current.
        let presses = controls.presses();
        let keys = controls.menu();
        // Typing a name or waiting for a new key takes the keyboard.
        let typing = app.name_edit.is_some()
            || app.rebinding.is_some()
            || app.screen == Screen::Join
            || app.chat.typing.is_some();
        if let Some(b) = app.rebinding {
            if let Some(k) = get_last_key_pressed() {
                app.settings_msg = match settings::code_of(k) {
                    Some(code) => app.settings.rebind(b, code),
                    None => "That key can't be bound. Pick another key.".to_owned(),
                };
                app.rebinding = None;
                app.save_settings();
            }
        }
        // The Join screen's address: an IP address or host name.
        if app.screen == Screen::Join {
            let before = app.join_addr.clone();
            for ch in typed_chars() {
                // One ':' and digits after it for a server on its own port.
                let port_ok = ch == ':' && !app.join_addr.contains(':')
                    || ch.is_ascii_digit() && app.join_addr.contains(':');
                let host_ok = (ch.is_ascii_alphanumeric() || ch == '.' || ch == '-')
                    && !app.join_addr.contains(':');
                if (host_ok || port_ok) && app.join_addr.len() < 64 {
                    app.join_addr.push(ch);
                }
            }
            if is_key_pressed(KeyCode::Backspace) {
                app.join_addr.pop();
            }
            // A changed address means a different server: stop looking at
            // the old one.
            if app.join_addr != before {
                app.lookup = None;
                if let Some(mut on) = app.online.take() {
                    on.leave();
                }
            }
            app.poll_lookup(&mut audio);
            if !app.join_addr.is_empty()
                && (is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter))
            {
                app.act(Act::Join, &mut audio);
            }
        }
        // Online chat: Enter opens it in a shift (the lobby has a button),
        // Enter sends the line and Escape drops it.
        if let Some(t) = app.chat.typing.as_mut() {
            for ch in typed_chars() {
                if !ch.is_control() && t.chars().count() < cj_net::CHAT_MAX {
                    t.push(ch);
                }
            }
            if is_key_pressed(KeyCode::Backspace) {
                t.pop();
            }
            if is_key_pressed(KeyCode::Escape) {
                app.chat.typing = None;
            } else if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter) {
                let line = app.chat.typing.take().unwrap_or_default();
                if let Some(on) = app.online.as_mut() {
                    if let Some(msg) = on.session.say(&line) {
                        on.send(msg);
                    }
                }
            }
        } else if !typing
            && app.online.is_some()
            && app.screen == Screen::Play
            && (is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter))
        {
            while get_char_pressed().is_some() {}
            app.chat.typing = Some(String::new());
        }
        if let Some(t) = app.name_edit.as_mut() {
            for ch in typed_chars() {
                if !ch.is_control() && t.chars().count() < profile::NAME_MAX * 2 {
                    t.push(ch);
                }
            }
            if is_key_pressed(KeyCode::Backspace) {
                t.pop();
            }
            if is_key_pressed(KeyCode::Escape) {
                app.name_edit = None;
            } else if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter) {
                app.act(Act::EditName, &mut audio);
            }
        }
        if !typing {
            if presses.mute {
                audio.toggle_mute();
                if audio.is_muted() {
                    app.voice.stop();
                }
            }
            if presses.voice {
                app.act(Act::ToggleVoice, &mut audio);
            }
            if presses.music {
                app.act(Act::SetMusic, &mut audio);
            }
        }
        if presses.skip_training && app.screen == Screen::Play && !typing {
            app.skip_training();
        }
        if app.settings_dirty {
            app.settings_dirty = false;
            look::set_colorblind(app.settings.colorblind);
            look::set_sprites(app.settings.sprites);
            look::set_reduce_motion(app.settings.reduce_motion);
            audio.set_volumes(app.settings.volume_gains());
            audio.set_mono(app.settings.mono);
            if app.settings.fullscreen != fullscreen_now {
                fullscreen_now = app.settings.fullscreen;
                set_fullscreen(fullscreen_now);
            }
            app.music.enabled = app.settings.music;
            controls.bind(&app.settings);
        }
        app.bench.update(
            get_time(),
            &app.sound_groups
                .iter()
                .flat_map(|g| &g.1)
                .collect::<Vec<_>>(),
            &mut audio,
            &mut app.voice,
        );
        let menu_in = ui::MenuInput {
            nav: if typing {
                None
            } else {
                app.ui.repeat(keys.dir)
            },
            activate: keys.activate && !typing,
            tab: keys.tab,
            // Escape / pad B back out of a screen. Typing keeps Escape for
            // itself (a rebind cancels, a name edit drops), except on Join,
            // whose address field is always open.
            back: keys.back
                && (!typing || (app.screen == Screen::Join && app.chat.typing.is_none())),
            start: keys.start,
        };

        // The real frame time, before the simulation's clamp, for F3.
        perf.record(get_frame_time());
        if is_key_pressed(KeyCode::F3) {
            perf.on = !perf.on;
        }

        let sim_began = std::time::Instant::now();
        // Simulation, or the drifting field behind the menus.
        let dt = f64::from(get_frame_time()).min(MAX_FRAME_DT);
        // The press that opens the pause card must not also close it below.
        let mut just_paused = false;
        // Online, the connection is pumped every frame, whatever is on screen.
        if let Some(on) = app.online.as_mut() {
            on.pump(dt);
            let now = get_time();
            for line in on.session.chat.drain(..) {
                app.chat.push(line, now);
            }
            if let Some(why) = on.ended.take() {
                app.leave_online(why);
            }
        }
        // A hosted server that failed has already told everyone; say why.
        if let Some(why) = app.hosted.as_ref().and_then(host::Hosted::failed) {
            app.leave_online(format!("Your server stopped after an error: {why}"));
        }
        app.follow_server(&mut audio);
        app.hint_if_alone();
        match app.screen {
            Screen::Play if app.online.is_some() => {
                // While chatting the keys are typing, not flying or pausing.
                if presses.pause && !smoke && !typing {
                    audio.set_thrust(0.0, 0.0, false);
                    app.open(Screen::Paused);
                    just_paused = true;
                } else {
                    let flying = app.chat.typing.is_none();
                    online_steps(
                        &mut app,
                        &presses,
                        &mut controls,
                        &mut audio,
                        &mut fx_rng,
                        &mut sm,
                        flying,
                    );
                }
            }
            // The shared shift goes on behind the pause card and its
            // settings: the tug is sent nothing held.
            Screen::Paused | Screen::Settings if app.online.is_some() => {
                online_steps(
                    &mut app,
                    &presses,
                    &mut controls,
                    &mut audio,
                    &mut fx_rng,
                    &mut sm,
                    false,
                );
            }
            Screen::Play => {
                if (presses.pause && !smoke) || step_away {
                    audio.set_thrust(0.0, 0.0, false);
                    app.open(Screen::Paused);
                    just_paused = true;
                } else {
                    play_steps(
                        &mut app,
                        &presses,
                        &mut controls,
                        &mut audio,
                        &mut fx_rng,
                        smoke,
                        &mut sm,
                    );
                    if app.world.shift_over {
                        app.end_shift(&mut audio);
                    }
                }
            }
            Screen::Paused => {}
            // Settings opened from the pause card sit over the shift in
            // progress: the menu drift below would move its cargo and clock.
            Screen::Settings if app.menu_from == Screen::Paused => {}
            _ => {
                app.world.step_ambient(dt);
                app.fx.update(dt);
                audio.set_thrust(0.0, 0.0, false);
            }
        }

        // The field's ambience plays only while flying.
        let field = if app.screen == Screen::Play {
            audio::field_ambience(&app.world)
        } else {
            audio::FieldSound::default()
        };
        audio.set_field(field);
        let echo = if app.screen == Screen::Play {
            audio::field_echo(&app.world)
        } else {
            0.0
        };
        audio.set_echo(echo);

        // CPU time only (the GPU runs behind), smoothed so F3 is readable.
        let smooth = |avg: &mut f32, t: std::time::Instant| {
            *avg += (t.elapsed().as_secs_f32() * 1000.0 - *avg) * 0.1;
        };
        smooth(&mut sim_ms, sim_began);
        let draw_began = std::time::Instant::now();
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
            show_tug: !matches!(
                app.screen,
                Screen::Title | Screen::Launch | Screen::Join | Screen::Lobby
            ),
            playing: app.screen == Screen::Play,
            settings: &app.settings,
            paint: app.profile.paint_colours(),
            post: &mut post,
            effects: app.settings.effects,
        });
        if matches!(app.screen, Screen::Play | Screen::Paused) {
            // Edge arrows point at world positions: real window units.
            radar::indicators(&look, &app.world, (vw, vh), dpi);
            // Everything else draws in HUD units through a camera that
            // magnifies them by the player's HUD size.
            // Capped so the HUD always keeps at least a 1280x720 layout:
            // any bigger and its cards start to overlap.
            let hs = app
                .settings
                .hud_scale()
                .min((vw / 1280.0).min(vh / 720.0).max(1.0));
            let hud_view = (vw / hs, vh / hs);
            look::set_hud_scale(hs);
            draw::screen_camera(hud_view);
            let blips = app.online.as_ref().map_or(&[][..], |o| o.session.blips());
            radar::radar(&app.world, hud_view, (vw, vh), blips);
            let real = app.world.me().banked;
            app.fx.banked_shown = hud::count_up(app.fx.banked_shown, real, get_frame_time().into());
            hud::draw(
                &look,
                &app.world,
                app.career.as_ref(),
                hud_view,
                controls.last_device,
                app.fx.banked_shown,
            );
            // In training, Marge's line repeats the card's: show it once.
            let card = app.world.tut.as_ref().map(|t| {
                cj_sim::tutorial::TUTORIAL[t.step.min(cj_sim::tutorial::TUTORIAL.len() - 1)]
            });
            app.radio.draw(&look, app.world.t, hud_view, card);
            if let Some(on) = app.online.as_ref() {
                let pilots = app.world.pilots.iter().filter(|p| p.away.is_none()).count();
                hud::online_badge(&look, hud_view, pilots, on.rtt_ms());
            }
            // Online, holding Tab shows who is flying and how they're doing.
            if let Some(on) = app.online.as_ref().filter(|_| is_key_down(KeyCode::Tab)) {
                online::player_list(&look, on.session.lobby.as_ref(), on.session.pilot, hud_view);
            }
            if app.online.is_some() {
                app.chat.draw(&look, get_time(), hud_view);
            }
            if marge::draw_tutorial(&look, &app.world, hud_view) && app.screen == Screen::Play {
                app.skip_training();
            }
            look::set_hud_scale(1.0);
            draw::screen_camera((vw, vh));
        }
        let scene = match app.screen {
            Screen::Play => music::Scene::Play,
            Screen::Paused => music::Scene::Paused,
            _ => music::Scene::Menu,
        };
        app.music
            .update(&audio, &app.world, scene, get_time(), get_frame_time());
        let picked = match app.screen {
            Screen::Play => None,
            Screen::Paused => {
                // Online the shift goes on underneath, so it's hidden.
                if app.online.is_some() {
                    ui::splash(&look, (vw, vh), dpi);
                } else {
                    ui::backdrop((vw, vh));
                }
                let mut menu = menu_in;
                menu.back = presses.pause && !just_paused; // P or Esc resumes
                screens::pause(
                    &look,
                    &mut app.ui,
                    &menu,
                    (vw, vh),
                    dpi,
                    (app.armed_end, app.armed_menu),
                    app.voice.enabled,
                    app.online.as_ref().map(|_| app.hosted.is_some()),
                )
            }
            Screen::Launch => {
                ui::backdrop((vw, vh));
                screens::launch(
                    &look,
                    &mut app.ui,
                    &menu_in,
                    (vw, vh),
                    dpi,
                    app.host_limit,
                    app.host_mode,
                    app.host_pvp,
                )
            }
            Screen::Join => {
                ui::backdrop((vw, vh));
                // Enter joins (handled with the typing above), not the
                // focused button.
                let mut menu = menu_in;
                menu.activate = false;
                let (status, can_join, looking) = app.join_status();
                let v = screens::JoinView {
                    address: &app.join_addr,
                    status,
                    can_join,
                    looking,
                    recent: &app.settings.recent_servers,
                };
                screens::join(&look, &mut app.ui, &menu, (vw, vh), dpi, v)
            }
            Screen::Lobby => {
                ui::splash(&look, (vw, vh), dpi);
                let session = app.online.as_ref().map(|o| &o.session);
                let v = screens::LobbyView {
                    lobby: session.and_then(|s| s.lobby.as_ref()),
                    waiting: session.is_some_and(|s| s.waiting),
                    ready: app.ready_sent,
                    can_shop: app.online_career.is_some(),
                };
                let act = screens::lobby(&look, &mut app.ui, &menu_in, (vw, vh), dpi, v);
                app.chat.draw(&look, get_time(), (vw, vh));
                act
            }
            Screen::Title => {
                ui::backdrop((vw, vh));
                let v = screens::TitleView {
                    pilot: &app.pilot,
                    editing: app.name_edit.as_deref(),
                    name_msg: &app.name_msg,
                    saved: app.saved.as_ref(),
                    mode: app.mode,
                    best: app.bests.get(app.mode),
                    tab: &mut app.title_tab,
                    armed_new: app.armed_new,
                };
                screens::title(&look, &mut app.ui, &menu_in, (vw, vh), dpi, v)
            }
            Screen::Report => {
                if app.online.is_some() {
                    ui::splash(&look, (vw, vh), dpi);
                } else {
                    ui::backdrop((vw, vh));
                }
                let rep = app.report.as_ref().expect("a report");
                screens::report(&look, &mut app.ui, &menu_in, (vw, vh), dpi, rep)
            }
            Screen::Shop => {
                // Online the shop shows the server's copy of this player's
                // career, never the single-player one.
                let career = if app.online.is_some() {
                    ui::splash(&look, (vw, vh), dpi);
                    app.online_career.as_ref()
                } else {
                    ui::backdrop((vw, vh));
                    app.career.as_ref()
                };
                match career {
                    Some(career) => {
                        let v = screens::ShopView {
                            career,
                            profile: &app.profile,
                            tab: &mut app.shop_tab,
                            msg: &app.shop_msg,
                            t: get_time() as f32,
                        };
                        screens::shop(&look, &mut app.ui, &menu_in, (vw, vh), dpi, v, &mut fx_rng)
                    }
                    None => Some(Act::ToMenu),
                }
            }
            Screen::Settings => {
                if app.online.is_some() {
                    ui::splash(&look, (vw, vh), dpi);
                } else {
                    ui::backdrop((vw, vh));
                }
                let v = screens::SettingsView {
                    settings: &app.settings,
                    rebinding: app.rebinding,
                    msg: &app.settings_msg,
                };
                screens::settings(&look, &mut app.ui, &menu_in, (vw, vh), dpi, v)
            }
            Screen::Records => {
                ui::backdrop((vw, vh));
                screens::records(&look, &mut app.ui, &menu_in, (vw, vh), dpi, &app.profile)
            }
            Screen::Board => {
                ui::backdrop((vw, vh));
                let day = daily_day();
                let key = match app.board_tab {
                    3 => format!("daily-{day}"),
                    i => cj_sim::career::MODES[i].def().key.to_owned(),
                };
                let rows = profile::board_for(&app.board, &key);
                let v = screens::BoardView {
                    rows: &rows,
                    tab: app.board_tab,
                    pilot: &app.pilot,
                    day: &day,
                };
                screens::board(&look, &mut app.ui, &menu_in, (vw, vh), dpi, v)
            }
            Screen::Sounds => {
                ui::backdrop((vw, vh));
                screens::sounds(
                    &look,
                    &mut app.ui,
                    &menu_in,
                    (vw, vh),
                    dpi,
                    &app.sound_groups,
                    app.bench.lit.map(|l| l.0),
                    &app.bench.now_playing,
                )
            }
        };
        if let Some(warning) = app.store.take_error() {
            app.say(warning, SAVE_WARNING_TIME);
        }
        app.next_toast();
        draw_toast(&look, &app.toast, (vw, vh));
        // Last, over every screen, in window units.
        smooth(&mut draw_ms, draw_began);
        if perf.on {
            draw::screen_camera((vw, vh));
            let mut extra = vec![format!("cpu sim {sim_ms:.2} ms  draw {draw_ms:.2} ms")];
            if let Some(on) = app.online.as_ref() {
                extra.extend(on.net_rows());
            }
            perf.draw(
                &look,
                (vw, vh),
                app.fx.particles.len(),
                app.settings.effects,
                &extra,
            );
        }
        let picked = if smoke { sm.drive(&mut app) } else { picked };
        if let Some(a) = picked {
            app.act(a, &mut audio);
        }
        // F12: a screenshot into the save folder (the game writes nowhere
        // else), named by the time so shots never overwrite each other.
        if is_key_pressed(KeyCode::F12) && !smoke {
            if let Some(dir) = app.store.folder().map(|d| d.join("screenshots")) {
                let secs = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs());
                let path = dir.join(format!("claim-jumpers-{secs}.png"));
                let msg = match std::fs::create_dir_all(&dir) {
                    Ok(()) => {
                        save_screen(&path);
                        format!("Screenshot saved to {}", path.display())
                    }
                    Err(e) => format!("Could not save a screenshot: {e}"),
                };
                app.say(msg, TOAST_TIME);
            }
        }
        if smoke && sm.capture(&app, start) {
            let passed = sm.finish(&app, &audio, get_time() - start);
            // The check's save folder was its own; leave nothing behind.
            if let Some(dir) = app.store.folder() {
                let _ = std::fs::remove_dir_all(dir);
            }
            std::process::exit(if passed { 0 } else { 1 });
        }
        if app.quit {
            break;
        }
        next_frame().await;
    }
}

/// Run this frame's fixed simulation steps (frame() in src/js/19-boot.js).
// The frame loop's own state, passed in from main(); one call site.
#[allow(clippy::too_many_arguments)]
fn play_steps(
    app: &mut App,
    presses: &controls::Presses,
    controls: &mut controls::Controls,
    audio: &mut audio::Audio,
    rng: &mut Rng,
    smoke: bool,
    sm: &mut Smoke,
) {
    // One-shot cable commands land before the next step, as the browser's
    // key handlers run between frames.
    // PvP needs a server, so single player ignores its key.
    for a in presses.actions.iter().filter(|a| **a != Action::TogglePvp) {
        app.world.action(*a);
    }
    if smoke {
        smoke_script(&mut app.world, sm.frame);
        // A highlight for the report's replay box.
        if sm.frame == 60 {
            app.recorder
                .note(900.0, "Smoke-test highlight", app.world.t);
        }
    }
    let input = if smoke {
        smoke_input(sm.frame)
    } else {
        controls.sample()
    };
    app.world.p2_input = controls.p2();
    // Fixed-step; a long stall drops the backlog instead of spiralling.
    let n_max = if smoke { 1 } else { MAX_STEPS_PER_FRAME };
    let real = if smoke {
        STEP
    } else {
        f64::from(get_frame_time()).min(MAX_FRAME_DT)
    };
    // Hit-stop freezes the simulation and slow motion slows it (simTime);
    // both only change how much time the fixed steps catch up on.
    app.accumulator += if app.hitstop_t > 0.0 {
        app.hitstop_t -= real;
        0.0
    } else if app.slow_t > 0.0 {
        app.slow_t -= real;
        real * cj_sim::consts::SLOWMO_RATE
    } else {
        real
    };
    let mut n = 0;
    while app.accumulator >= STEP && n < n_max && !app.world.shift_over {
        let training = app.world.tut.is_some();
        app.world.step(&input, STEP);
        // Only finishing it reaches here; a skip is handled by skip_training.
        if training && app.world.tut.is_none() {
            app.store.save_training_done();
            app.unlock("trained", audio);
        }
        audio.tick(STEP);
        handle_events(app, audio, controls, rng, sm, false);
        app.recorder
            .record(&app.world, STEP, app.fx.particles.len());
        app.fx.update_trails(&app.world);
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

/// Plays this step's (or, online, this frame's) events: particles, sound,
/// radio lines, rumble, highlights and achievements; then clears them.
fn handle_events(
    app: &mut App,
    audio: &mut audio::Audio,
    controls: &mut controls::Controls,
    rng: &mut Rng,
    sm: &mut Smoke,
    online: bool,
) {
    app.fx.absorb(app.world.events.iter().map(|n| &n.event));
    // Another player's sound comes from their tug: quieter with distance.
    let me = app.world.me().ship.b;
    let gain = |to: Option<usize>| match to {
        Some(q) if q != app.world.view_pilot => app.world.pilots.get(q).map_or(0.0, |pl| {
            let d = cj_sim::js::dist(me.x, me.y, pl.ship.b.x, pl.ship.b.y);
            (1.0 - d / cj_net::HEARING).max(0.0) as f32
        }),
        _ => 1.0,
    };
    let heard: Vec<_> = app
        .world
        .events
        .iter()
        .map(|n| (gain(n.to), n.event.clone(), n.at))
        .collect();
    for (g, e, at) in &heard {
        match e {
            Event::Sfx(s) => {
                let (pan, muffle) = audio::place_of(*s, *at, (me.x, me.y));
                audio.sfx_from(*s, *g, pan, muffle);
                sm.sfx += 1;
                // Your own milestones get a musical sting too.
                let sting = match s {
                    Sfx::Bank => Some(music::Sting::Bank),
                    Sfx::Jackpot => Some(music::Sting::Jackpot),
                    Sfx::WantedUp => Some(music::Sting::Wanted),
                    _ => None,
                };
                if let Some(st) = sting.filter(|_| *g >= 1.0) {
                    app.music.sting(audio, st);
                }
            }
            Event::Thrust {
                level,
                load,
                sputtering,
            } => audio.set_thrust(*level, *load, *sputtering),
            Event::Radio {
                key,
                force,
                once,
                text,
            } => {
                if *key == cj_sim::events::Radio::Boss {
                    app.music.sting(audio, music::Sting::Boss);
                }
                app.radio.line(
                    *key,
                    *force,
                    *once,
                    text.as_deref(),
                    app.world.t,
                    rng,
                    audio,
                    &mut app.voice,
                )
            }
            // Online, nobody else's world freezes, so neither does yours.
            Event::HitStop(s) if !online => app.hitstop_t = app.hitstop_t.max(*s),
            Event::SlowMo(s) if !online => app.slow_t = app.slow_t.max(*s),
            Event::Highlight { score, text } => app.recorder.note(*score, text, app.world.t),
            Event::Rumble { strength, ms } => controls.rumble(*strength, *ms),
            _ => {}
        }
    }
    let unlocks: Vec<&'static str> = app
        .world
        .events
        .iter()
        .map(|n| &n.event)
        .filter_map(|e| match e {
            Event::Unlock(a) => Some(a.id()),
            _ => None,
        })
        .collect();
    for id in unlocks {
        app.unlock(id, audio);
    }
    app.world.events.clear();
}

/// A frame of an online shift. Cable commands and each fixed step's held
/// controls go to the server (nothing held when `flying` is false, behind
/// a menu), the client's own tug is guessed forward, and the world is
/// redrawn from the server's snapshots.
fn online_steps(
    app: &mut App,
    presses: &controls::Presses,
    controls: &mut controls::Controls,
    audio: &mut audio::Audio,
    rng: &mut Rng,
    sm: &mut Smoke,
    flying: bool,
) {
    let real = f64::from(get_frame_time()).min(MAX_FRAME_DT);
    let Some(on) = app.online.as_mut() else {
        return;
    };
    if flying {
        for a in &presses.actions {
            let msg = on.session.action(*a);
            on.send(msg);
        }
    }
    let input = if flying {
        controls.sample()
    } else {
        Input::default()
    };
    app.accumulator += real;
    let mut n = 0;
    while app.accumulator >= STEP && n < MAX_STEPS_PER_FRAME {
        if let Some(msg) = on.session.step(input) {
            on.send(msg);
        }
        audio.tick(STEP);
        app.fx.update(STEP);
        app.accumulator -= STEP;
        n += 1;
    }
    if n == MAX_STEPS_PER_FRAME {
        app.accumulator = 0.0;
    }
    on.flush();
    on.session.present(&mut app.world, real);
    handle_events(app, audio, controls, rng, sm, true);
    app.fx.update_trails(&app.world);
    app.fx.emit_ambient(&app.world);
}

/* ------------------------------ Smoke run ------------------------------ */

/// Frames of scripted flight before the shift is cut short.
const SMOKE_FRAMES: u32 = 150;
/// Screenshots a complete tour takes (the names in `Smoke::capture`).
const SMOKE_SCREENS: usize = 12;

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
            // End shift now takes two presses: arm it, then press again.
            Screen::Paused if leave || self.on_screen == 5 => Some(Act::EndShiftNow),
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
            // After the title: settings, records, the board and the sound
            // test, each captured and left with Back.
            Screen::Title if leave && self.shots.len() >= 7 => match self.shots.len() {
                8 => Some(Act::OpenSettings),
                9 => Some(Act::OpenRecords),
                10 => Some(Act::OpenBoard),
                _ => Some(Act::OpenSounds),
            },
            Screen::Sounds if self.on_screen == 2 => Some(Act::PlaySound(0)),
            Screen::Settings | Screen::Records | Screen::Board if leave => Some(Act::Back),
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
            Screen::Settings if self.on_screen == 3 => "settings",
            Screen::Records if self.on_screen == 3 => "records",
            Screen::Board if self.on_screen == 3 => "board",
            Screen::Sounds if self.on_screen == 3 => "sounds",
            _ => return false,
        };
        let path = std::env::temp_dir().join(format!("claim-jumpers-m5-{name}.png"));
        save_screen(&path);
        if name == "play" {
            let w = &app.world;
            let s = &w.me().ship;
            self.lines.push(format!(
                "  play: {} frames, {} steps in {:.2} s; tug ({:.1}, {:.1}) hull {:.1} shield {:.1}; chain {} ({:.1} t); {} shots, {} jumpers, {} crabs, {} rocks",
                self.frame, self.steps, get_time() - start, s.b.x, s.b.y, s.hp, s.shield,
                w.me().chain.len(), w.tow_mass(0), w.me().shots_fired, w.jumpers.len(), w.crabs.len(), w.rocks.len()
            ));
            self.lines.push(format!(
                "  opposition: wanted {}, {} law units, {} collectors, {} nets, {} miners, refinery {:?} with {} drones, {} missiles in flight ({} in the rack), {} sentries",
                w.me().wanted, w.sheriffs.len(), w.collectors.len(), w.nets.len(), w.miners.len(),
                w.refinery.as_ref().map(|r| r.state), w.drones.len(), w.missiles.len(), s.missiles, w.sentries.len()
            ));
            self.lines.push(format!(
                "  field: {} under {}; ion storm {:.1} s; boss {:?}; hauler {:?} hp; sealed holds {}; keycards {}; contract {:?} at {}",
                w.belt.def().name, w.cond.def().name, w.ion_t,
                w.jumpers.iter().find_map(|j| j.boss.as_ref().map(|b| b.name)),
                w.hauler.as_ref().map(|h| h.hp),
                w.wrecks.iter().filter(|x| x.locked).count(),
                w.items.iter().filter(|i| i.kind == ItemType::Keycard).count(),
                w.me().contract.as_ref().map(|k| k.text()),
                w.me().contract.as_ref().map_or(0.0, |k| w.contract_progress(0, k))
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
        name == "sounds"
    }

    /// Print the report; true if the tour did what it should have. The
    /// scripted shift ends solvent, so a working save holds shift 2.
    fn finish(&self, app: &App, audio: &audio::Audio, secs: f64) -> bool {
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
            saved.as_ref().map_or("none".to_owned(), |c| format!(
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
            "  radio: {} lines shown; last: {}",
            app.radio.lines_shown(),
            app.radio.text
        );
        println!(
            "  voice: {} (speech off for the check run)",
            app.voice.status
        );
        println!("  training done: {}", app.store.training_done());
        println!(
            "  profile: {} shift(s), {} achievement(s) {:?}; pilot {}; board rows {}; sound test: {}",
            app.profile.stats[0],
            app.profile.got.len(),
            app.profile.got.iter().map(|g| g.0).collect::<Vec<_>>(),
            app.pilot,
            app.board.len(),
            app.bench.now_playing
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
        let career_saved = saved.is_some_and(|c| c.shift == 2);
        let all_screens = self.shots.len() == SMOKE_SCREENS;
        if !career_saved {
            println!("SMOKE FAILED: the career on disk is not at shift 2");
        }
        if !all_screens {
            println!(
                "SMOKE FAILED: {} of {SMOKE_SCREENS} screens captured",
                self.shots.len()
            );
        }
        career_saved && all_screens
    }
}

/// Smoke script: put a piece of ore beside the tug and hook it, with a
/// rival cutter and a riding crab close by so combat is on screen.
fn smoke_script(w: &mut World, frame: u32) {
    if frame == 1 {
        // Start well clear of the bay, or the bay banks the ore at once.
        w.me_mut().ship.b.x = BAY_X - 500.0;
        let it = w.new_item(ItemType::Ore, BAY_X - 500.0, BAY_Y + 110.0, 0.0, 0.0);
        let host = it.id;
        w.items.push(it);
        w.me_mut().ship.invuln = 0.0;
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
        w.me_mut().banked = 1840.0;
        // The opposition on screen: a missile rack and drone bay with three
        // sentries, the refinery close by (its drones wake), a debt
        // collector, a mining crew, and two wanted stars bringing the law.
        let mut mounted = [None; 5];
        mounted[0] = Some(cj_sim::parts::Part::Cannon);
        mounted[1] = Some(cj_sim::parts::Part::Missiles);
        mounted[3] = Some(cj_sim::parts::Part::DroneBay);
        w.me_mut().mods =
            cj_sim::mods::compute_mods(&mounted, &cj_sim::parts::Levels::default(), 0);
        for i in 0..3 {
            let s = w.make_sentry(0, i, 3);
            w.sentries.push(s);
        }
        w.spawn_refinery();
        if let Some(r) = w.refinery.as_mut() {
            r.b.x = BAY_X - 450.0;
            r.b.y = BAY_Y - 650.0;
        }
        w.spawn_collector(0);
        if let Some(c) = w.collectors.last_mut() {
            c.b.x = BAY_X - 150.0;
            c.b.y = BAY_Y + 450.0;
        }
        w.spawn_miners(2);
        for (i, m) in w.miners.iter_mut().enumerate() {
            m.b.x = BAY_X - 1000.0 + i as f64 * 60.0;
            m.b.y = BAY_Y + 250.0;
        }
        let (x, y) = (w.me().ship.b.x, w.me().ship.b.y);
        w.add_wanted(0, 2, x, y);
        w.reinforce_t = Some(0.0);
        // The field systems: a named belt and condition, a contract on the
        // HUD, an ion storm, a boss crew and an escorted hauler close by,
        // and a sealed hold with its keycard.
        w.belt = cj_sim::field::Belt::Kessler;
        w.cond = cj_sim::field::Condition::IonFront;
        w.me_mut().contract = Some(cj_sim::field::make_contract(
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

/// Saves what the window shows now as a PNG at `path`.
fn save_screen(path: &std::path::Path) {
    // get_screen_data comes back top row first, as the window shows it,
    // but carries the alpha of the last blend; make it opaque.
    let mut img = get_screen_data();
    for px in img.bytes.chunks_mut(4) {
        px[3] = 255;
    }
    img.export_png(&path.to_string_lossy());
}

/// This frame's typed characters, plus the clipboard's text on Ctrl+V (an
/// address or a name is often copied from a chat window). Each field still
/// filters what it accepts, character by character.
fn typed_chars() -> Vec<char> {
    let mut out: Vec<char> = std::iter::from_fn(get_char_pressed).collect();
    let ctrl = is_key_down(KeyCode::LeftControl) || is_key_down(KeyCode::RightControl);
    if ctrl && is_key_pressed(KeyCode::V) {
        if let Some(text) = miniquad::window::clipboard_get() {
            out.extend(text.trim().chars());
        }
    }
    out
}

/// The achievement toast (#toast): top centre, a hazard edge, 3.5 s.
fn draw_toast(look: &look::Look, toast: &Option<(String, f64)>, view: (f32, f32)) {
    let Some((text, until)) = toast else { return };
    if get_time() > *until {
        return;
    }
    // A long message wraps rather than running off a narrow window.
    let font = &look.fonts.body;
    let width = |t: &str| look::text_width(t, 14.0, font);
    let lines = online::wrap(text, (view.0 - 92.0).clamp(200.0, 720.0), width);
    let w = lines.iter().map(|l| width(l)).fold(0.0, f32::max) + 28.0;
    let h = 34.0 + 20.0 * (lines.len().max(1) - 1) as f32;
    let x = (view.0 - w) / 2.0;
    draw_rectangle(
        x,
        110.0,
        w,
        h,
        Color::new(10.0 / 255.0, 14.0 / 255.0, 24.0 / 255.0, 0.92),
    );
    draw_rectangle_lines(x, 110.0, w, h, 1.0, look::hex(look::col::HAZARD));
    for (i, l) in lines.iter().enumerate() {
        let y = 118.0 + 20.0 * i as f32;
        look::text_left(l, x + 14.0, y, 14.0, font, look::hex(look::col::TEXT));
    }
}

/// Milliseconds since the epoch (Date.now()), for achievement and score times.
fn now_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_millis() as f64)
}

/// dailyDay: today's date in UTC, YYYY-MM-DD.
fn daily_day() -> String {
    civil_date((now_ms() / 86_400_000.0).floor() as i64)
}

/// The date `days` after 1970-01-01, as YYYY-MM-DD. Civil-from-days (Howard
/// Hinnant), valid for the Gregorian calendar.
fn civil_date(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// seedOf: the FNV-1a hash of a string, the daily seed.
fn seed_of(s: &str) -> u32 {
    s.encode_utf16().fold(2_166_136_261u32, |h, ch| {
        (h ^ u32::from(ch)).wrapping_mul(16_777_619)
    })
}

/// Today's daily field and the fixed loadout everyone flies it with,
/// generated from the date's seed (beginDaily inside withDailySeed).
fn daily_world(day: &str, view: (f64, f64)) -> (World, Career) {
    use cj_sim::field::{Belt, Event as FieldEvent};
    use cj_sim::parts::{Part, System};
    const DAILY_BELTS: [Belt; 4] = [Belt::Quiet, Belt::Hollis, Belt::Kessler, Belt::Cinder];
    const DAILY_EVENTS: [FieldEvent; 4] = [
        FieldEvent::Ion,
        FieldEvent::Derelict,
        FieldEvent::Boss,
        FieldEvent::Convoy,
    ];
    let mut rng = Rng::seeded(seed_of(&format!("claim-jumpers:{day}")));
    // The fixed loadout every daily pilot flies.
    let mut c = Career::new(Mode::Standard);
    c.shift = 5;
    c.loan_start = 5;
    c.tug_class = 1;
    c.sentries = 2;
    c.owned = vec![
        Part::Cannon,
        Part::Twin,
        Part::Missiles,
        Part::Winch,
        Part::DroneBay,
    ];
    c.mounted = [
        Some(Part::Cannon),
        Some(Part::Missiles),
        Some(Part::Twin),
        Some(Part::Winch),
        Some(Part::DroneBay),
    ];
    for s in [System::Shield, System::Thrust, System::Armor] {
        c.levels.set(s, 2);
    }
    c.belt = DAILY_BELTS[rng.index(DAILY_BELTS.len())];
    c.ensure_conditions(&mut rng);
    let mut w = World::for_career(rng, view, &c);
    w.event_t = w.rng.range(70.0, 140.0);
    w.event_done = false;
    w.forced_event = Some(DAILY_EVENTS[w.rng.index(DAILY_EVENTS.len())]);
    (w, c)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The daily challenge is the same field twice (the browser's test).
    #[test]
    fn the_daily_field_is_the_same_twice() {
        let layout = |w: &World| {
            let rocks: Vec<(i64, i64)> = w
                .rocks
                .iter()
                .take(8)
                .map(|r| (r.b.x.round() as i64, r.b.y.round() as i64))
                .collect();
            format!(
                "{:?} {:?} {:?} {:?} {}",
                w.belt, w.cond, w.forced_event, rocks, w.event_t
            )
        };
        let view = (1280.0, 720.0);
        let (a, ca) = daily_world("2026-11-07", view);
        let (b, _) = daily_world("2026-11-07", view);
        assert_eq!(layout(&a), layout(&b));
        assert_eq!(ca.tug_class, 1);
        let (c, _) = daily_world("2026-11-08", view);
        assert_ne!(layout(&a), layout(&c), "another day, another field");
    }

    #[test]
    fn daily_seed_and_day_match_the_browser() {
        // seedOf("abc") in the browser's FNV-1a.
        assert_eq!(seed_of("abc"), 0x1a47_e90b);
        let d = daily_day();
        assert_eq!(d.len(), 10);
        assert_eq!(&d[4..5], "-");
    }

    #[test]
    fn civil_dates_are_right_across_month_and_leap_boundaries() {
        assert_eq!(civil_date(0), "1970-01-01");
        assert_eq!(civil_date(-1), "1969-12-31");
        assert_eq!(civil_date(19_782), "2024-02-29");
        assert_eq!(civil_date(19_783), "2024-03-01");
        assert_eq!(civil_date(20_088), "2024-12-31");
        assert_eq!(civil_date(20_089), "2025-01-01");
    }
}
