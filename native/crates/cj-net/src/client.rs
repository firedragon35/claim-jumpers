//! A client's side of the protocol (CJN-16), with no window or socket: the
//! game wraps it in a renet transport, and the server's tests drive it
//! directly.
//!
//! The client predicts only its own tug: every fixed step it applies its
//! held controls at once (`World::predict_step`) and sends them numbered.
//! When a snapshot arrives, the prediction restarts from the server's state
//! and replays the controls the server hasn't applied yet (the snapshot's
//! `ack`), so the tug feels immediate while the server stays the authority.
//! The client's own shots are guessed too: each is shown the moment it is
//! fired, from the predicted tug, and flies on locally until it would hit
//! something, while the server's copy (which decides what it really hits)
//! is hidden. The sounds and puffs of the tug's own flight play at once.
//! Everything else is drawn about 100 ms behind the newest snapshot,
//! blended between the two around that moment, so it moves smoothly
//! between snapshots that arrive 20 times a second.

use crate::snapshot::{Look, Snapshot};
use crate::{
    channel, ChatLine, ClientMsg, DecodeError, Info, Lobby, Phase, PlayerId, Reject, ServerMsg,
};
use cj_sim::consts::STEP;
use cj_sim::entities::{Body, Bullet, Crater, ShapePt};
use cj_sim::events::{Emitted, Event, Sfx};
use cj_sim::rng::Rng;
use cj_sim::world::World;
use cj_sim::Input;
use std::collections::{HashMap, VecDeque};

/// How far behind the newest snapshot others are drawn, in steps (100 ms):
/// two snapshots' worth, so one lost snapshot doesn't stall them.
const INTERP_DELAY: f64 = 6.0;
/// Inputs kept for replay; beyond this the server is far behind and the
/// oldest are dropped (about two seconds).
const HISTORY_MAX: usize = 120;

/// Feedback from the tug's own flight that the client plays as it
/// predicts, instead of waiting a round trip for the server's copy: the
/// engine, side-thruster puffs, and the sound of its own shots.
fn is_local(e: &Event) -> bool {
    matches!(
        e,
        Event::Thrust { .. } | Event::Strafe { .. } | Event::Sfx(Sfx::Shoot(_) | Sfx::Slug)
    )
}

/// Something drawn between snapshots: matched by id, blended by position
/// and (when it has one) heading.
trait Tracked {
    fn id(&self) -> u32;
    fn pos(&mut self) -> (&mut f64, &mut f64);
    fn angle(&mut self) -> Option<&mut f64>;
}

/// Turns from `a` toward `b` by `t` (0..1) the short way round, so a
/// heading crossing the ±π seam doesn't spin the long way.
fn lerp_angle(a: f64, b: f64, t: f64) -> f64 {
    use std::f64::consts::{PI, TAU};
    let d = (b - a + PI).rem_euclid(TAU) - PI;
    a + d * t
}

macro_rules! tracked_body {
    ($($t:ty),*) => {$(
        impl Tracked for $t {
            fn id(&self) -> u32 { self.id }
            fn pos(&mut self) -> (&mut f64, &mut f64) { (&mut self.b.x, &mut self.b.y) }
            fn angle(&mut self) -> Option<&mut f64> { Some(&mut self.angle) }
        }
    )*};
}
tracked_body!(
    cj_sim::entities::Item,
    cj_sim::entities::Rock,
    cj_sim::entities::Jumper,
    cj_sim::opposition::Drone,
    cj_sim::opposition::Collector,
    cj_sim::opposition::Sheriff,
    cj_sim::opposition::Miner
);
// The two x/y entities: a sentry turns, a net doesn't.
impl Tracked for cj_sim::opposition::Sentry {
    fn id(&self) -> u32 {
        self.id
    }
    fn pos(&mut self) -> (&mut f64, &mut f64) {
        (&mut self.x, &mut self.y)
    }
    fn angle(&mut self) -> Option<&mut f64> {
        Some(&mut self.angle)
    }
}
impl Tracked for cj_sim::opposition::Net {
    fn id(&self) -> u32 {
        self.id
    }
    fn pos(&mut self) -> (&mut f64, &mut f64) {
        (&mut self.x, &mut self.y)
    }
    fn angle(&mut self) -> Option<&mut f64> {
        None
    }
}

/// Moves each entity in `now` back toward its place in `before` (matched
/// by id): `alpha` 0 is `before`, 1 is `now`. Entities only in `now` stay.
fn blend<T: Tracked + Clone>(now: &mut [T], before: &[T], alpha: f64) {
    let old: HashMap<u32, T> = before.iter().map(|e| (e.id(), e.clone())).collect();
    for e in now {
        if let Some(mut o) = old.get(&e.id()).cloned() {
            let (ox, oy) = o.pos();
            let (ox, oy) = (*ox, *oy);
            let (x, y) = e.pos();
            *x = ox + (*x - ox) * alpha;
            *y = oy + (*y - oy) * alpha;
            let oa = o.angle().map(|a| *a);
            if let (Some(oa), Some(a)) = (oa, e.angle()) {
                *a = lerp_angle(oa, *a, alpha);
            }
        }
    }
}

/// The login details a [`Session`] holds until its Hello goes out.
struct Login {
    game_version: String,
    player: PlayerId,
    print: crate::auth::Print,
    pilot: String,
    view: (u16, u16),
    hello_sent: bool,
}

/// A connection's state as the client sees it.
pub struct Session {
    /// The pilot the server gave this client, once welcomed.
    pub pilot: Option<usize>,
    /// Why the server refused this client, if it did.
    pub rejected: Option<Reject>,
    /// The server's answer to a [`Session::query`].
    pub info: Option<Info>,
    /// Accepted, but waiting in the lobby for the next shift.
    pub waiting: bool,
    /// The newest lobby list (players, scores, the shift or break).
    pub lobby: Option<Lobby>,
    /// Chat lines received, waiting for the game to take them.
    pub chat: Vec<ChatLine>,
    /// The newest shop answer: whether it went through, its sentence, and
    /// this player's career now, waiting for the game to take it.
    pub shop: Option<(bool, String, cj_sim::career::Career)>,
    /// The last shift's settlement and this player's career after it,
    /// waiting for the game to take them.
    pub settled: Option<(cj_sim::career::Settlement, cj_sim::career::Career)>,
    /// The server's login challenge, once it has arrived: the Hello
    /// can't be built before it (see [`crate::auth`]).
    nonce: Option<[u8; 16]>,
    /// The server asked for this player's print (a first join there).
    introduce: bool,
    /// Who this client logs in as ([`Session::set_login`]).
    login: Option<Login>,
    /// The current shift's first server step: older snapshots are dropped.
    from_tick: u32,
    seq: u32,
    action_seq: u32,
    /// Controls sent but not yet applied by the server, oldest first.
    history: VecDeque<(u32, Input)>,
    /// Outlines received, by rock and item id.
    rock_looks: HashMap<u32, (Vec<ShapePt>, Vec<Crater>)>,
    item_looks: HashMap<u32, Vec<ShapePt>>,
    /// The two newest snapshots, for blending.
    older: Option<Snapshot>,
    newer: Option<Snapshot>,
    /// A snapshot arrived that the prediction hasn't restarted from yet.
    fresh: bool,
    /// The moment drawn, in server steps.
    render_tick: f64,
    /// The world the prediction runs in: the newest snapshot, plus this
    /// client's tug guessed forward.
    pred: World,
    /// Events received, waiting for the game.
    events: Vec<Emitted>,
    /// This client's own shots, as guessed when fired (see the module notes).
    shots: Vec<Bullet>,
    /// Feedback from the steps just predicted (see [`is_local`]).
    own_feedback: Vec<Emitted>,
}

impl Default for Session {
    fn default() -> Self {
        Session::new()
    }
}

impl Session {
    pub fn new() -> Session {
        Session {
            pilot: None,
            rejected: None,
            info: None,
            waiting: false,
            lobby: None,
            chat: Vec::new(),
            settled: None,
            shop: None,
            nonce: None,
            introduce: false,
            login: None,
            from_tick: 0,
            seq: 0,
            action_seq: 0,
            history: VecDeque::new(),
            rock_looks: HashMap::new(),
            item_looks: HashMap::new(),
            older: None,
            newer: None,
            fresh: false,
            render_tick: 0.0,
            pred: World::empty(Rng::Constant(0.5), (1280.0, 720.0)),
            events: Vec::new(),
            shots: Vec::new(),
            own_feedback: Vec::new(),
        }
    }

    /// Sets who this client logs in as. Nothing is sent yet: the Hello
    /// needs the server's challenge, so [`Session::take_login`] hands it
    /// out once that has arrived. Only the secret's print is kept.
    pub fn set_login(
        &mut self,
        game_version: &str,
        player: PlayerId,
        secret: &[u8; 16],
        pilot: &str,
        view: (u16, u16),
    ) {
        self.login = Some(Login {
            game_version: game_version.to_owned(),
            player,
            print: crate::auth::fingerprint(secret),
            pilot: pilot.to_owned(),
            view,
            hello_sent: false,
        });
    }

    /// The next login message to send (reliable), if one is due: the Hello
    /// once the challenge is in, proving the secret without sending it;
    /// then, only if a server new to this player asks, the print. Call it
    /// after taking in messages, until it returns `None`.
    pub fn take_login(&mut self) -> Option<(u8, Vec<u8>)> {
        let login = self.login.as_mut()?;
        let msg = if std::mem::take(&mut self.introduce) {
            ClientMsg::Introduce { print: login.print }
        } else if !login.hello_sent {
            let nonce = self.nonce?;
            login.hello_sent = true;
            ClientMsg::Hello {
                game_version: login.game_version.clone(),
                player: login.player,
                proof: crate::auth::proof(&nonce, &login.player, &login.print),
                pilot: login.pilot.clone(),
                view: login.view,
            }
        } else {
            return None;
        };
        Some((channel::RELIABLE, crate::encode(&msg)))
    }

    /// A [`ClientMsg::Query`] to send before Hello (reliable).
    pub fn query(&self) -> (u8, Vec<u8>) {
        (channel::RELIABLE, crate::encode(&ClientMsg::Query))
    }

    /// Tells the server this player is ready for the next shift (reliable).
    pub fn ready(&self) -> (u8, Vec<u8>) {
        (channel::RELIABLE, crate::encode(&ClientMsg::Ready))
    }

    /// A public chat line to send (reliable), or None if nothing is left
    /// once it's cleaned.
    pub fn say(&self, text: &str) -> Option<(u8, Vec<u8>)> {
        let text = crate::clean_chat(text);
        (!text.is_empty()).then(|| (channel::RELIABLE, crate::encode(&ClientMsg::Chat { text })))
    }

    /// A shop request or belt vote to send (reliable).
    pub fn shop_req(&self, req: crate::ShopReq) -> (u8, Vec<u8>) {
        (channel::RELIABLE, crate::encode(&ClientMsg::Shop(req)))
    }

    /// Whether the server is between shifts. A break list from before
    /// this client's newest Welcome is old news.
    /// Inputs sent but not yet confirmed by a snapshot (replayed on top
    /// of each one): a growing number means the server is falling behind.
    pub fn unconfirmed(&self) -> usize {
        self.history.len()
    }

    pub fn in_break(&self) -> bool {
        self.lobby
            .as_ref()
            .is_some_and(|l| matches!(l.phase, Phase::Break { .. }) && l.tick >= self.from_tick)
    }

    /// Forgets everything about the last shift's world (a new one has
    /// started, or this client is waiting for one). The message numbering
    /// carries on, as the server's does.
    fn reset_shift(&mut self) {
        self.history.clear();
        self.rock_looks.clear();
        self.item_looks.clear();
        self.older = None;
        self.newer = None;
        self.fresh = false;
        self.render_tick = 0.0;
        self.pred = World::empty(Rng::Constant(0.5), (1280.0, 720.0));
        self.events.clear();
        self.shots.clear();
        self.own_feedback.clear();
    }

    /// Takes in one message from the server.
    pub fn receive(&mut self, bytes: &[u8]) -> Result<(), DecodeError> {
        match crate::decode::<ServerMsg>(bytes)? {
            ServerMsg::Welcome { pilot, from_tick } => {
                self.reset_shift();
                self.pilot = Some(usize::from(pilot));
                self.from_tick = from_tick;
                self.waiting = false;
            }
            ServerMsg::Waiting => {
                self.reset_shift();
                self.pilot = None;
                self.waiting = true;
            }
            ServerMsg::Info(i) => self.info = Some(i),
            ServerMsg::Challenge { nonce } => self.nonce = Some(nonce),
            ServerMsg::Introduce => self.introduce = true,
            ServerMsg::Chat(line) => self.chat.push(line),
            ServerMsg::Shop(r) => {
                if let Some(c) = cj_sim::career::Career::from_json(&r.career) {
                    self.shop = Some((r.ok, r.msg, c));
                }
            }
            ServerMsg::Settled(s) => {
                // A career the game can't read is the server's mistake; the
                // report is skipped rather than shown wrong.
                if let Some(c) = cj_sim::career::Career::from_json(&s.career) {
                    self.settled = Some((s.settlement, c));
                }
            }
            ServerMsg::Lobby(l) => {
                if self.lobby.as_ref().is_none_or(|o| l.tick >= o.tick) {
                    self.lobby = Some(l);
                }
            }
            ServerMsg::Reject(r) => self.rejected = Some(r),
            ServerMsg::Snapshot(s) => {
                // Snapshots travel unreliably and may arrive out of order,
                // or from the last shift after the new one's Welcome.
                //
                // A new shift's snapshot can also arrive before its Welcome:
                // the Welcome travels in its own (reliable) packet, and a
                // lost one is resent 300 ms later. Until it lands this client
                // still holds last shift's pilot number, which the new shift
                // may have given to someone else or dropped (players who left
                // give up their slot, so the rest move down). Flying that
                // number in the new field would index a pilot that isn't
                // there, so a snapshot made for another pilot is dropped; the
                // next one after the Welcome is taken. The count check keeps
                // a server that sends nonsense from doing the same.
                let mine = self.pilot == Some(usize::from(s.you)) && s.you < s.pilot_count;
                if mine
                    && s.tick >= self.from_tick
                    && self.newer.as_ref().is_none_or(|n| s.tick > n.tick)
                {
                    self.older = self.newer.take();
                    self.newer = Some(*s);
                    self.fresh = true;
                }
            }
            ServerMsg::Events(e) => self.events.extend(e),
            ServerMsg::Looks(looks) => {
                for l in looks {
                    match l {
                        Look::Rock { id, shape, craters } => {
                            self.rock_looks.insert(id, (shape, craters));
                        }
                        Look::Item { id, shape } => {
                            self.item_looks.insert(id, shape);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// One fixed step of local play: guesses this client's tug forward
    /// with `input` and returns the message to send (on the unreliable
    /// channel), or None before the server has welcomed this client.
    pub fn step(&mut self, input: Input) -> Option<(u8, Vec<u8>)> {
        let p = self.pilot.filter(|_| self.newer.is_some())?;
        self.restart_prediction();
        self.seq += 1;
        self.history.push_back((self.seq, input));
        if self.history.len() > HISTORY_MAX {
            self.history.pop_front();
        }
        let fired = self.pred.bullets.len();
        self.pred.predict_step(p, &input, STEP);
        // Shots fired this step show at once (a laser lands in the server's
        // step and is drawn from its event, so it isn't guessed).
        self.shots
            .extend(self.pred.bullets.drain(fired..).filter(|b| !b.laser));
        self.own_feedback
            .extend(self.pred.events.drain(..).filter(|e| is_local(&e.event)));
        for b in &mut self.shots {
            b.x += b.vx * STEP;
            b.y += b.vy * STEP;
            b.life -= STEP;
        }
        self.shots.retain(|b| b.life > 0.0);
        // The newest steps' controls, so the server still has each one if
        // the packet that first carried it was lost.
        let from = self.history.len().saturating_sub(crate::INPUTS_PER_MESSAGE);
        let inputs: Vec<Input> = self.history.iter().skip(from).map(|(_, i)| *i).collect();
        let msg = ClientMsg::Input {
            seq: self.seq,
            inputs,
        };
        Some((channel::UNRELIABLE, crate::encode(&msg)))
    }

    /// A cable command to send (reliable). The server applies it; the
    /// client waits to see the result rather than guessing it.
    pub fn action(&mut self, action: cj_sim::Action) -> (u8, Vec<u8>) {
        self.action_seq += 1;
        let msg = ClientMsg::Action {
            seq: self.action_seq,
            action,
        };
        (channel::RELIABLE, crate::encode(&msg))
    }

    /// After a new snapshot: start the prediction again from the server's
    /// state and replay the controls it hasn't applied yet.
    fn restart_prediction(&mut self) {
        let (Some(p), Some(snap)) = (self.pilot, self.newer.as_ref()) else {
            return;
        };
        if !self.fresh {
            return;
        }
        self.fresh = false;
        fill(&mut self.pred, snap, &self.rock_looks, &self.item_looks);
        while self
            .history
            .front()
            .is_some_and(|(seq, _)| *seq <= snap.ack)
        {
            self.history.pop_front();
        }
        for (_, input) in &self.history {
            self.pred.predict_step(p, input, STEP);
        }
        self.pred.events.clear();
    }

    /// Brings `w` up to date for drawing, `dt` seconds after the last call:
    /// everything else blended about 100 ms behind the newest snapshot, and
    /// this client's tug and cargo as predicted. Received events are handed
    /// over in `w.events`.
    pub fn present(&mut self, w: &mut World, dt: f64) {
        self.restart_prediction();
        let (Some(p), Some(newer)) = (self.pilot, self.newer.as_ref()) else {
            return;
        };
        // The drawn moment trails the newest snapshot, catching up (or
        // jumping, if far off) when snapshots come early or late.
        let target = f64::from(newer.tick) - INTERP_DELAY;
        self.render_tick += dt / STEP;
        if (self.render_tick - target).abs() > INTERP_DELAY * 2.0 {
            self.render_tick = target;
        }
        self.render_tick += (target - self.render_tick) * 0.1;
        fill(w, newer, &self.rock_looks, &self.item_looks);
        if let Some(older) = self.older.as_ref().filter(|o| o.tick < newer.tick) {
            let span = f64::from(newer.tick - older.tick);
            let alpha = ((self.render_tick - f64::from(older.tick)) / span).clamp(0.0, 1.0);
            blend(&mut w.items, &older.items, alpha);
            blend(&mut w.rocks, &older.rocks, alpha);
            blend(&mut w.jumpers, &older.jumpers, alpha);
            blend(&mut w.drones, &older.drones, alpha);
            blend(&mut w.collectors, &older.collectors, alpha);
            blend(&mut w.sheriffs, &older.sheriffs, alpha);
            blend(&mut w.miners, &older.miners, alpha);
            blend(&mut w.sentries, &older.sentries, alpha);
            blend(&mut w.nets, &older.nets, alpha);
            // Other pilots' tugs, by slot.
            for o in &older.others {
                if let Some(pl) = w.pilots.get_mut(usize::from(o.slot)) {
                    let b = &mut pl.ship.b;
                    b.x = o.ship.b.x + (b.x - o.ship.b.x) * alpha;
                    b.y = o.ship.b.y + (b.y - o.ship.b.y) * alpha;
                    // Without this a spinning remote tug turns in 20 Hz steps.
                    pl.ship.angle = lerp_angle(o.ship.angle, pl.ship.angle, alpha);
                }
            }
        }
        // This client's tug and the cargo on its cable, as predicted.
        if let Some(mine) = self.pred.pilots.get(p) {
            w.pilots[p] = mine.clone();
            for id in &mine.chain {
                let guess = self.pred.items.iter().find(|it| it.id == *id);
                let shown = w.items.iter_mut().find(|it| it.id == *id);
                if let (Some(g), Some(s)) = (guess, shown) {
                    s.b = g.b;
                    s.angle = g.angle;
                }
            }
        }
        // This client's shots as guessed, in place of the server's copies; a
        // guessed shot that reaches something ends there (the server decides
        // what it really did).
        // (Sentry rounds carry this pilot's blame too, but aren't guessed.)
        let me = cj_sim::world::Blame::Pilot(p);
        w.bullets.retain(|b| b.blame != me || b.sentry);
        self.shots.retain(|b| !strikes(w, b));
        w.bullets.extend(self.shots.iter().cloned());
        w.view_pilot = p;
        w.update_camera(dt);
        // The tug's own feedback plays as predicted; the server's later
        // copies of it are dropped.
        w.events.append(&mut self.own_feedback);
        w.events.extend(
            self.events
                .drain(..)
                .filter(|e| !(e.to == Some(p) && is_local(&e.event))),
        );
    }

    /// What the newest snapshot reports beyond the view, for the radar.
    pub fn blips(&self) -> &[crate::Blip] {
        self.newer.as_ref().map_or(&[], |s| &s.blips[..])
    }

    /// This client's own shots in flight, as guessed.
    pub fn shots(&self) -> &[Bullet] {
        &self.shots
    }
}

/// Copies a snapshot into `w`, restoring outlines from the looks received.
/// The camera, view and events are left alone.
fn fill(
    w: &mut World,
    s: &Snapshot,
    rock_looks: &HashMap<u32, (Vec<ShapePt>, Vec<Crater>)>,
    item_looks: &HashMap<u32, Vec<ShapePt>>,
) {
    w.t = s.t;
    w.time_left = s.time_left;
    w.shift_over = s.shift_over;
    w.shift_no = s.shift_no;
    w.belt = s.belt;
    w.cond = s.cond;
    w.grudge = s.grudge;
    w.ion_t = s.ion_t;
    w.reinforce_t = s.reinforce_t;
    w.event_t = s.event_t;
    w.event_done = s.event_done;
    w.edge_lo = s.edge_lo;
    w.edge_hi = s.edge_hi;
    // Every slot gets a pilot; those out of sight are parked placeholders,
    // marked not flying so nothing draws them.
    let you = usize::from(s.you);
    w.pilots = (0..usize::from(s.pilot_count))
        .map(|q| {
            if q == you {
                return s.me.clone();
            }
            let mut pl = cj_sim::world::Pilot::new(s.me.mods.clone());
            pl.ship.alive = false;
            pl
        })
        .collect();
    for o in &s.others {
        if let Some(pl) = w.pilots.get_mut(usize::from(o.slot)) {
            pl.ship = o.ship.clone();
            pl.chain = o.chain.clone();
            pl.pvp = if o.pvp {
                cj_sim::perm::Pvp::Enabled
            } else {
                cj_sim::perm::Pvp::Protected
            };
        }
    }
    w.items = s.items.clone();
    for it in &mut w.items {
        if let Some(shape) = item_looks.get(&it.id) {
            it.shape = shape.clone();
        }
    }
    w.rocks = s.rocks.clone();
    for r in &mut w.rocks {
        if let Some((shape, craters)) = rock_looks.get(&r.id) {
            r.shape = shape.clone();
            r.craters = craters.clone();
        }
    }
    w.wrecks = s.wrecks.clone();
    w.jumpers = s.jumpers.clone();
    w.crabs = s.crabs.clone();
    w.bullets = s.bullets.clone();
    w.ebullets = s.ebullets.clone();
    w.drones = s.drones.clone();
    w.collectors = s.collectors.clone();
    w.nets = s.nets.clone();
    w.sheriffs = s.sheriffs.clone();
    w.miners = s.miners.clone();
    w.missiles = s.missiles.clone();
    w.sentries = s.sentries.clone();
    w.clouds = s.clouds.clone();
    w.refinery = s.refinery.clone();
    w.hauler = s.hauler.clone();
}

/// Whether a guessed shot has reached something solid in the drawn world,
/// another player's tug included (it stops there, as on the server).
fn strikes(w: &World, b: &Bullet) -> bool {
    let hit = |o: &Body, pad: f64| cj_sim::js::dist(b.x, b.y, o.x, o.y) < o.r + pad;
    let shooter = match b.blame {
        cj_sim::world::Blame::Pilot(p) => Some(p),
        _ => None,
    };
    w.pilots
        .iter()
        .enumerate()
        .any(|(q, pl)| Some(q) != shooter && pl.ship.alive && hit(&pl.ship.b, 3.0))
        || w.rocks.iter().any(|r| hit(&r.b, 0.0))
        || w.wrecks.iter().any(|r| hit(&r.b, 10.0))
        || w.jumpers.iter().any(|j| hit(&j.b, 4.0))
        || w.drones.iter().any(|d| hit(&d.b, 4.0))
        || w.miners.iter().any(|m| hit(&m.b, 4.0))
        || w.sheriffs.iter().any(|s| hit(&s.b, 4.0))
        || w.collectors.iter().any(|c| hit(&c.b, 4.0))
}

#[cfg(test)]
mod tests {
    /// Headings blend the short way across the ±π seam.
    #[test]
    fn angles_blend_the_short_way_round() {
        use std::f64::consts::PI;
        let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
        assert!(close(super::lerp_angle(0.0, 1.0, 0.5), 0.5));
        // 170° to -170° passes through 180°, not through 0°.
        let a = super::lerp_angle(PI - 0.2, -PI + 0.2, 0.5);
        assert!(close(a.rem_euclid(2.0 * PI), PI), "{a}");
        assert!(close(super::lerp_angle(2.0, -1.0, 1.0), -1.0));
    }

    use super::*;
    use crate::snapshot::Known;
    use cj_sim::career::{Career, Mode};

    const VIEW: (f64, f64) = (1280.0, 720.0);

    /// A field with `pilots` tugs at the bay and nothing else.
    fn field(pilots: usize) -> World {
        let mut w = World::empty(Rng::Constant(0.5), VIEW);
        // The first career takes over the field's own waiting tug.
        for _ in 0..pilots {
            w.add_career_pilot(&Career::new(Mode::Standard));
        }
        w
    }

    /// The server's snapshot of `w` for pilot `you` at step `tick`, encoded.
    fn snapshot(w: &World, you: usize, tick: u32) -> Vec<u8> {
        let (s, _) = Snapshot::of(w, you, VIEW, tick, 0, &mut Known::default());
        crate::encode(&ServerMsg::Snapshot(Box::new(s)))
    }

    fn welcome(pilot: u8, from_tick: u32) -> Vec<u8> {
        crate::encode(&ServerMsg::Welcome { pilot, from_tick })
    }

    /// Regression: a new shift's snapshot that arrived before its Welcome
    /// (lost and resent) was taken with last shift's pilot number. When a
    /// player had left, the others had moved down a slot, so that number was
    /// past the new shift's pilots and the next step crashed the game. Such
    /// a snapshot is now dropped until the Welcome names this client's pilot.
    #[test]
    fn a_new_shifts_snapshot_before_its_welcome_is_dropped() {
        let mut s = Session::new();
        let mut seen = World::empty(Rng::Constant(0.5), VIEW);
        // Last shift: three pilots, and this client flew the third.
        s.receive(&welcome(2, 0)).expect("valid");
        s.receive(&snapshot(&field(3), 2, 100)).expect("valid");
        assert!(s.step(Input::default()).is_some(), "flying");
        // The next shift has two pilots and this client is now the second,
        // but its Welcome is late and a snapshot is here first.
        s.receive(&snapshot(&field(2), 1, 200)).expect("valid");
        assert!(s.step(Input::default()).is_some(), "still the old shift");
        s.present(&mut seen, STEP);
        assert_eq!(seen.pilots.len(), 3, "the early snapshot was dropped");
        // The Welcome lands, and the snapshots after it are taken.
        s.receive(&welcome(1, 150)).expect("valid");
        s.receive(&snapshot(&field(2), 1, 203)).expect("valid");
        assert!(s.step(Input::default()).is_some(), "flying the new shift");
        s.present(&mut seen, STEP);
        assert_eq!((seen.pilots.len(), seen.view_pilot), (2, 1));
    }

    /// A snapshot naming a pilot past its own pilot count (a broken or
    /// hostile server) is dropped rather than flown.
    #[test]
    fn a_snapshot_for_a_pilot_it_doesnt_hold_is_dropped() {
        let mut s = Session::new();
        s.receive(&welcome(4, 0)).expect("valid");
        let (mut snap, _) = Snapshot::of(&field(2), 1, VIEW, 10, 0, &mut Known::default());
        snap.you = 4;
        let bytes = crate::encode(&ServerMsg::Snapshot(Box::new(snap)));
        s.receive(&bytes).expect("valid");
        assert!(s.step(Input::default()).is_none(), "nothing to fly");
    }
}
