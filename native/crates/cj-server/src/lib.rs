//! The Claim Jumpers authoritative server (CJN-15).
//!
//! A [`Server`] owns the one shared [`World`] and decides everything in it.
//! Clients only send their held controls and cable commands; the server
//! validates them, steps the world at the simulation's fixed 60 Hz, and
//! sends each client a snapshot of what its pilot may see 20 times a
//! second, with the events that client may hear.
//!
//! Shifts follow one another. When one ends the server holds a break: it
//! ends when every connected player is ready, or after 90 s, and the next
//! shift starts on a fresh field. A newcomer in a shift's last minute (or
//! the break) waits in the lobby and gets a pilot when the next shift
//! starts. Players who are gone at the break give up their slot. Everyone
//! connected is sent the lobby list (names, this shift's scores, the shift
//! or break) every second.
//!
//! The server works on a [`RenetServer`] and never touches sockets itself:
//! the headless binary drives it over UDP (renet_netcode), the game's host
//! uses renet's in-memory local client, and the tests shuttle packets by
//! hand. Every path goes through the same validation, so the host player
//! gets no privileged route into the world. (A player who hosts still runs
//! the server process and can change anything in it.)

pub mod careers;

use cj_net::{
    channel, ClientMsg, Info, Known, Lobby, LobbyPlayer, Phase, PlayerId, Reject, ServerMsg,
    Snapshot,
};
use cj_sim::career::{Career, Mode, Outcome};
use cj_sim::consts::STEP;
use cj_sim::field::{roll_conditions, Belt, Condition, BELTS};
use cj_sim::rng::Rng;
use cj_sim::world::{SharedShift, World};
use cj_sim::{Action, Input};
use renet::{ClientId, ConnectionConfig, RenetServer, ServerEvent};
use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant, SystemTime};

/// Simulation steps between snapshots: 60 Hz / 3 = 20 snapshots a second.
pub const SNAPSHOT_EVERY: u32 = 3;
/// Steps a new connection has to say Hello before it is dropped (5 s).
const HELLO_TIMEOUT: u32 = 5 * 60;
/// Steps a rejected client keeps its connection, so the reason reaches it.
const REJECT_LINGER: u32 = 30;
/// Invalid messages a client may send before it is disconnected.
const MAX_STRIKES: u32 = 10;
/// Held-control messages a client may send per second (3 per step at 60 Hz);
/// more are dropped unread.
const MAX_INPUTS_PER_SECOND: u32 = 180;
/// Cable commands a client may send per second; more are dropped.
const MAX_ACTIONS_PER_SECOND: u32 = 20;
/// Steps of controls the server holds for one pilot, waiting to be applied
/// one per step. More means the client is ahead (a burst after a stall),
/// and the oldest are dropped so the delay never grows.
const INPUT_QUEUE_MAX: usize = 8;
/// Chat lines a client may send per second; more are dropped.
const MAX_CHATS_PER_SECOND: u32 = 2;
/// The window, in steps (one minute), in which at most `max_players` players
/// new to the careers folder are let in. Each newcomer writes a key and a
/// career to disk, flushed on the step loop's thread, and one who waits in
/// the lobby frees their slot by leaving, so without a cap a script could
/// repeat that without end: filling the folder and stalling every shift.
/// shortcut: a cap slows a flood but can't stop a patient one or the
/// mid-shift squatting of slots (an away pilot keeps theirs until the
/// break); stopping those needs authenticated clients, if hosts ever face it.
const NEW_ID_WINDOW: u32 = 60 * 60;
/// The longest break between shifts, in steps (90 s).
pub const BREAK_STEPS: u32 = 90 * 60;
/// Steps between lobby lists in a shift (1 s) and in the break (0.25 s).
const LOBBY_EVERY: u32 = 60;
const LOBBY_EVERY_BREAK: u32 = 15;
/// Spreads the seeds of successive shifts (the golden ratio in 32 bits).
const SEED_STEP: u32 = 0x9e37_79b9;
/// The window a server assumes for its own world (only the sim's camera
/// uses it; each client's sight comes from its Hello).
const SERVER_VIEW: (f64, f64) = (1280.0, 720.0);

/// How a server is set up.
#[derive(Clone, Debug)]
pub struct Config {
    /// The game version clients must match exactly.
    pub version: String,
    /// Pilots in one shift, 1 to [`MAX_PLAYERS`]; it also sizes the field.
    pub max_players: usize,
    /// Seeds the first shift's field (later shifts derive theirs from it),
    /// so a recording can replay a shift.
    pub seed: u32,
    /// Keep a [`Recording`] of every join, input and command in the
    /// current shift.
    pub record: bool,
    /// The difficulty mode of every career on this server; it also sets
    /// how fast rivals arrive.
    pub mode: Mode,
    /// The folder the players' careers are kept in, or `None` to keep them
    /// only while the server runs (see [`careers`]).
    pub careers: Option<std::path::PathBuf>,
    /// The host allows PvP: players may switch it on in the bay (off by
    /// default, owner decision 2026-11-11).
    pub pvp: bool,
}

/// One entry of a [`Recording`].
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Entry {
    /// A new pilot joined the shift, flying this career (in the save
    /// format).
    Join { career: String },
    /// Pilot `pilot`'s held controls changed.
    Input { pilot: usize, input: Input },
    /// Pilot `pilot` sent a cable command.
    Action { pilot: usize, action: Action },
    /// Pilot `pilot`'s player left; the tug is out of the field.
    Away { pilot: usize },
    /// Pilot `pilot`'s player came back.
    Back { pilot: usize },
}

/// Everything that fed a shift, by the step it was applied before:
/// replaying it reproduces the shift exactly (see [`replay`]). It is saved
/// with `cj_net::encode`.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Recording {
    pub seed: u32,
    /// The player limit, which sized the field.
    pub limit: usize,
    /// What the server settled for the whole shift.
    pub shared: Option<SharedShift>,
    /// The careers the shift started with, in pilot order (save format).
    pub start: Vec<String>,
    pub entries: Vec<(u32, Entry)>,
}

/// One player's place in the shift. Its index in `Server::players` is its
/// pilot's index in the world, for the whole shift: a player who leaves
/// keeps the slot (their tug waits out of the field) and reclaims it on
/// return, until the break. Players waiting in the lobby are kept apart, in
/// `Server::waiting`.
struct Player {
    id: PlayerId,
    secret: [u8; 16],
    name: String,
    view: (f64, f64),
    conn: Option<ClientId>,
    /// The controls applied at the last step, and that step's number.
    input: Input,
    input_seq: u32,
    /// Controls received for the coming steps, one per step, in order, and
    /// the newest step number queued.
    queue: std::collections::VecDeque<(u32, Input)>,
    queued_seq: u32,
    action_seq: u32,
    known: Known,
    /// Events raised since the last snapshot that this player may hear.
    pending: Vec<cj_sim::events::Emitted>,
    /// Ready for the next shift (in the break).
    ready: bool,
    /// The career this player flies on this server.
    career: Career,
    /// Their vote for the next shift's belt, in the break.
    vote: Option<Belt>,
    /// The shop has changed `career` since it was last saved (see
    /// [`Server::save_changed`]).
    unsaved: bool,
}

impl Player {
    fn new(id: PlayerId, secret: [u8; 16], career: Career) -> Player {
        Player {
            id,
            secret,
            career,
            vote: None,
            name: String::new(),
            view: (0.0, 0.0),
            conn: None,
            input: Input::default(),
            input_seq: 0,
            queue: std::collections::VecDeque::new(),
            queued_seq: 0,
            action_seq: 0,
            known: Known::default(),
            pending: Vec::new(),
            ready: false,
            unsaved: false,
        }
    }
}

/// Whether a shift is on or the server is between shifts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Shift,
    /// The break, which ends by step `until` at the latest.
    Break {
        until: u32,
    },
}

/// What a connection is doing.
enum ConnState {
    /// Connected; its Hello hasn't arrived yet.
    Joining,
    /// Flying `pilot`.
    Playing { pilot: usize },
    /// In the lobby, waiting for the next shift.
    Waiting,
    /// Refused; dropped at step `at` once the reason has had time to arrive.
    Leaving { at: u32 },
}

/// One connected client.
struct Conn {
    state: ConnState,
    /// The step it connected at.
    since: u32,
    /// Invalid messages so far.
    strikes: u32,
    /// Messages this second, for the rate limits.
    inputs: u32,
    actions: u32,
    chats: u32,
}

/// The authoritative server for one shared shift.
pub struct Server {
    /// The connections; drive it with a transport (or local clients).
    pub renet: RenetServer,
    /// The shared world.
    pub world: World,
    config: Config,
    tick: u32,
    conns: HashMap<ClientId, Conn>,
    players: Vec<Player>,
    /// Players in the lobby for the next shift, in the order they came.
    waiting: Vec<Player>,
    stage: Stage,
    /// Shifts started, counting the current one.
    shift_no: u32,
    /// The step the current shift started at.
    shift_start: u32,
    /// What the current shift's field was built with.
    shared: SharedShift,
    /// The server's own dice, for what isn't the field's: each break's
    /// conditions and contract offers.
    rng: Rng,
    /// Every belt's conditions for the next shift, rolled at the break.
    conditions: [Condition; 5],
    careers: careers::CareerStore,
    /// Players new to the careers folder let in during this
    /// [`NEW_ID_WINDOW`], capped at `max_players`.
    new_ids: usize,
    recording: Option<Recording>,
    /// Lines worth logging since the last drain (joins, leaves, rejects).
    /// Nothing secret is ever written here.
    pub log: Vec<String>,
}

/// The most pilots one shift can hold.
pub const MAX_PLAYERS: usize = 20;

/// Seconds left in a shift below which no new pilot may join it.
pub const LATE_JOIN_CUTOFF: f64 = 60.0;

/// A shift's field: seeded with `seed`, sized for `limit` pilots, built
/// with `shared` for the whole field and `careers` for its first pilots.
fn new_world(seed: u32, limit: usize, shared: &SharedShift, careers: &[Career]) -> World {
    World::for_careers(Rng::seeded(seed), SERVER_VIEW, limit, shared, careers)
}

/// A server's first shift: the first belt in normal conditions, with the
/// host's PvP setting (kept for every later shift).
fn first_shift(mode: Mode, pvp: bool) -> SharedShift {
    SharedShift {
        belt: Belt::Quiet,
        cond: Condition::Normal,
        mode,
        shift_no: 1,
        grudge: 0.0,
        pvp,
    }
}

/// A career from its save text; recordings only hold careers the server
/// wrote, so one that doesn't load is a broken recording.
fn career_of(text: &str) -> Career {
    Career::from_json(text).expect("a recording holds careers the server saved")
}

/// The player behind connection `id`: flying (in `players`) or waiting in
/// the lobby (in `waiting`). None for a connection that isn't a player yet
/// (no Hello, or refused). It takes the lists rather than the whole server
/// so a caller can still use the server's other fields (connections, dice,
/// careers, log) while it holds the player.
fn player_of<'a>(
    conns: &HashMap<ClientId, Conn>,
    players: &'a mut [Player],
    waiting: &'a mut [Player],
    id: ClientId,
) -> Option<&'a mut Player> {
    match conns.get(&id)?.state {
        ConnState::Playing { pilot } => players.get_mut(pilot),
        ConnState::Waiting => waiting.iter_mut().find(|p| p.conn == Some(id)),
        ConnState::Joining | ConnState::Leaving { .. } => None,
    }
}

/// The belt votes of the players still connected, most first (ties in
/// sector-map order).
fn tally<'a>(players: impl Iterator<Item = &'a Player>) -> Vec<(Belt, u8)> {
    let mut n = [0u8; 5];
    for v in players.filter(|p| p.conn.is_some()).filter_map(|p| p.vote) {
        let at = BELTS.iter().position(|b| *b == v).expect("listed");
        n[at] = n[at].saturating_add(1);
    }
    let mut out: Vec<(Belt, u8)> = BELTS
        .iter()
        .copied()
        .zip(n)
        .filter(|(_, k)| *k > 0)
        .collect();
    out.sort_by_key(|(_, k)| std::cmp::Reverse(*k));
    out
}

/// The next shift's belt: the most-voted belt that every player's tug can
/// fly. A tie keeps the current belt if it is among the tied ones; with no
/// votes the current belt stays, unless someone can't fly it, when the
/// first belt everyone can fly is used.
fn winning_belt(players: &[Player], current: Belt) -> Belt {
    let everyone = |b: Belt| {
        players
            .iter()
            .all(|p| p.career.clone().pick_belt(b).is_ok())
    };
    let votes: Vec<(Belt, u8)> = tally(players.iter())
        .into_iter()
        .filter(|(b, _)| everyone(*b))
        .collect();
    match votes.first() {
        Some(&(_, top)) => {
            let tied: Vec<Belt> = votes
                .iter()
                .filter(|(_, k)| *k == top)
                .map(|(b, _)| *b)
                .collect();
            if tied.contains(&current) {
                current
            } else {
                tied[0]
            }
        }
        None if everyone(current) => current,
        None => BELTS
            .iter()
            .copied()
            .find(|b| everyone(*b))
            .unwrap_or(Belt::Quiet),
    }
}

/// Replays `rec` for `steps` steps on a fresh world, applying each entry
/// before the step it was recorded at, exactly as the server did.
pub fn replay(rec: &Recording, steps: u32) -> World {
    let shared = rec
        .shared
        .unwrap_or_else(|| first_shift(Mode::Standard, false));
    let start: Vec<Career> = rec.start.iter().map(|c| career_of(c)).collect();
    let mut w = new_world(rec.seed, rec.limit, &shared, &start);
    let mut inputs: Vec<Input> = vec![Input::default(); start.len()];
    let mut next = 0;
    for tick in 0..steps {
        while let Some((at, e)) = rec.entries.get(next).filter(|(at, _)| *at == tick) {
            debug_assert_eq!(*at, tick);
            match e {
                Entry::Join { career } => {
                    w.join_career_pilot(&career_of(career));
                    inputs.push(Input::default());
                }
                Entry::Input { pilot, input } => inputs[*pilot] = *input,
                Entry::Action { pilot, action } => w.action_for(*pilot, *action),
                Entry::Away { pilot } => w.set_away(*pilot),
                Entry::Back { pilot } => w.set_back(*pilot),
            }
            next += 1;
        }
        if !inputs.is_empty() {
            w.step_all(&inputs, STEP);
            w.events.clear();
        }
    }
    w
}

impl Server {
    /// A server for a new shift.
    pub fn new(config: Config) -> Server {
        let limit = config.max_players.clamp(1, MAX_PLAYERS);
        let shared = first_shift(config.mode, config.pvp);
        let (careers, warning) = careers::CareerStore::open(config.careers.clone());
        Server {
            renet: RenetServer::new(ConnectionConfig::default()),
            world: new_world(config.seed, limit, &shared, &[]),
            recording: config.record.then(|| Recording {
                seed: config.seed,
                limit,
                shared: Some(shared),
                start: Vec::new(),
                entries: Vec::new(),
            }),
            shared,
            careers,
            new_ids: 0,
            rng: Rng::seeded(config.seed ^ 0x5eed_5eed),
            conditions: [Condition::Normal; 5],
            log: warning.into_iter().collect(),
            config,
            tick: 0,
            conns: HashMap::new(),
            players: Vec::new(),
            waiting: Vec::new(),
            stage: Stage::Shift,
            shift_no: 1,
            shift_start: 0,
        }
    }

    /// Player `id`'s career on this server: the saved one, or a fresh one
    /// in the server's mode (saved at once, so it survives a restart).
    fn career_for(&mut self, id: &PlayerId) -> Career {
        let (saved, problem) = self.careers.load(id);
        self.log.extend(problem);
        saved.unwrap_or_else(|| {
            let c = Career::new(self.config.mode);
            if let Err(e) = self.careers.save(id, &c) {
                self.log.push(e);
            }
            c
        })
    }

    /// Saves every career the shop has changed since it was last saved.
    ///
    /// A save waits for the disk on the step loop's thread, so the shop
    /// doesn't save on each request: a client sending requests as fast as
    /// the rate limit allows (20 a second) would stall the server for
    /// everyone. The shop marks the career instead, and this runs once a
    /// second, when the next shift starts, when a lobby player leaves or is
    /// replaced (their copy in memory goes with them), and when the server
    /// stops. A server that dies without stopping can lose the last second
    /// of shopping, and nothing else.
    fn save_changed(&mut self) {
        let changed = self
            .players
            .iter_mut()
            .chain(&mut self.waiting)
            .filter(|p| p.unsaved);
        for p in changed {
            // Cleared even if the save fails: the fault is logged once, not
            // every second, and the next change or settlement tries again.
            p.unsaved = false;
            if let Err(e) = self.careers.save(&p.id, &p.career) {
                self.log.push(e);
            }
        }
    }

    /// Steps simulated so far.
    pub fn tick(&self) -> u32 {
        self.tick
    }

    /// Players in the shift (connected or not).
    pub fn player_count(&self) -> usize {
        self.players.len()
    }

    /// Players waiting in the lobby for the next shift.
    pub fn waiting_count(&self) -> usize {
        self.waiting.len()
    }

    /// Whether the server is in the break between shifts.
    pub fn in_break(&self) -> bool {
        matches!(self.stage, Stage::Break { .. })
    }

    /// Shifts started, counting the current one.
    pub fn shift_no(&self) -> u32 {
        self.shift_no
    }

    /// Every slot is taken, by players in the shift or the lobby.
    fn full(&self) -> bool {
        self.players.len() + self.waiting.len() >= self.config.max_players
    }

    /// The recording so far, if this server keeps one.
    pub fn recording(&self) -> Option<&Recording> {
        self.recording.as_ref()
    }

    /// Adds `e` to the recording, at the step of the shift it comes before.
    fn record(&mut self, e: Entry) {
        let tick = self.tick - self.shift_start;
        if let Some(r) = self.recording.as_mut() {
            r.entries.push((tick, e));
        }
    }

    /// One server step of `dt`: take in connections and messages, step the
    /// world, and every [`SNAPSHOT_EVERY`] steps send each client its view.
    /// Call it at 60 Hz; the transport moves packets before and after.
    pub fn update(&mut self, dt: Duration) {
        self.renet.update(dt);
        self.take_events();
        // In connection-id order, so who claims a slot first never depends
        // on hashing.
        let mut ids: Vec<ClientId> = self.conns.keys().copied().collect();
        ids.sort_unstable();
        for id in ids {
            self.take_messages(id);
        }
        self.expire_conns();
        match self.stage {
            Stage::Shift => {
                if !self.players.is_empty() {
                    self.take_inputs();
                    let inputs: Vec<Input> = self.players.iter().map(|p| p.input).collect();
                    self.world.step_all(&inputs, STEP);
                    self.share_events();
                }
                if self.world.shift_over {
                    self.start_break();
                }
            }
            Stage::Break { until } => {
                if self.tick >= until || self.all_ready() {
                    self.start_shift();
                }
            }
        }
        // In the break the last shift's final view stays up behind the
        // lobby list.
        if self.tick.is_multiple_of(SNAPSHOT_EVERY) {
            self.send_views();
        }
        let every = match self.stage {
            Stage::Shift => LOBBY_EVERY,
            Stage::Break { .. } => LOBBY_EVERY_BREAK,
        };
        if self.tick.is_multiple_of(every) {
            self.send_lobby();
        }
        self.tick += 1;
        if self.tick.is_multiple_of(60) {
            self.save_changed();
            for c in self.conns.values_mut() {
                c.inputs = 0;
                c.actions = 0;
                c.chats = 0;
            }
        }
        if self.tick.is_multiple_of(NEW_ID_WINDOW) {
            self.new_ids = 0;
        }
    }

    /// The shift has ended: each pilot's shift is settled into their
    /// career (saved, and sent to them), and the break begins, with
    /// everyone's controls let go and nobody ready yet. A career that ended
    /// (repossessed, or the fleet paid off) starts again fresh.
    fn start_break(&mut self) {
        self.stage = Stage::Break {
            until: self.tick + BREAK_STEPS,
        };
        // Every belt's conditions for the next shift, the same for everyone.
        self.conditions = roll_conditions(&mut self.rng, self.shift_no as i32 + 1);
        let mut out = Vec::new();
        for slot in 0..self.players.len() {
            let p = &mut self.players[slot];
            p.vote = None;
            let settlement = p.career.settle(&self.world, slot);
            if matches!(
                settlement.outcome,
                Outcome::Repossessed { .. } | Outcome::FleetPaid
            ) {
                p.career = Career::new(self.config.mode);
            }
            p.career.conditions = Some((p.career.shift, self.conditions));
            p.career.ensure_offers(&mut self.rng);
            if let Err(e) = self.careers.save(&p.id, &p.career) {
                self.log.push(e);
            }
            if let Some(id) = p.conn {
                let msg = ServerMsg::Settled(Box::new(cj_net::Settled {
                    settlement,
                    career: p.career.to_json(),
                }));
                out.push((id, msg));
            }
        }
        // Players waiting in the lobby shop for their first shift too. Their
        // roll is saved at once: leaving the lobby drops the copy in memory,
        // and a career reloaded without it would roll a fresh set.
        for p in &mut self.waiting {
            p.vote = None;
            p.career.conditions = Some((p.career.shift, self.conditions));
            p.career.ensure_offers(&mut self.rng);
            if let Err(e) = self.careers.save(&p.id, &p.career) {
                self.log.push(e);
            }
            if let Some(id) = p.conn {
                let msg = ServerMsg::Shop(cj_net::ShopReply {
                    ok: true,
                    msg: String::new(),
                    career: p.career.to_json(),
                });
                out.push((id, msg));
            }
        }
        for (id, msg) in out {
            self.send(id, channel::RELIABLE, &msg);
        }
        // The rivals' grudge carries over, as it does in a career.
        self.shared.grudge = self.world.grudge;
        for p in self.players.iter_mut().chain(self.waiting.iter_mut()) {
            p.ready = false;
            p.input = Input::default();
            p.queue.clear();
        }
        self.log
            .push(format!("shift {} over; break started", self.shift_no));
        self.send_lobby();
    }

    /// Everyone connected, flying or waiting, is ready (and someone is).
    fn all_ready(&self) -> bool {
        let mut connected = self
            .players
            .iter()
            .chain(&self.waiting)
            .filter(|p| p.conn.is_some())
            .peekable();
        connected.peek().is_some() && connected.all(|p| p.ready)
    }

    /// Starts the next shift on a fresh field: the players still connected
    /// keep flying in their order, then those from the lobby; players who
    /// are gone give up their slot. Each is welcomed to the new shift.
    fn start_shift(&mut self) {
        // Before the players who are gone give up their slots: their careers
        // in memory go with them.
        self.save_changed();
        let seed = self
            .config
            .seed
            .wrapping_add(self.shift_no.wrapping_mul(SEED_STEP));
        self.shift_no += 1;
        let limit = self.config.max_players.clamp(1, MAX_PLAYERS);
        let mut players: Vec<Player> = std::mem::take(&mut self.players)
            .into_iter()
            .filter(|p| p.conn.is_some())
            .collect();
        players.append(&mut self.waiting);
        self.shared.shift_no = self.shift_no as i32;
        let belt = winning_belt(&players, self.shared.belt);
        let at = BELTS.iter().position(|b| *b == belt).expect("listed");
        self.shared.belt = belt;
        self.shared.cond = self.conditions[at];
        for p in &mut players {
            p.career.belt = belt;
            p.vote = None;
        }
        let start: Vec<Career> = players.iter().map(|p| p.career.clone()).collect();
        self.world = new_world(seed, limit, &self.shared, &start);
        self.stage = Stage::Shift;
        self.shift_start = self.tick;
        if self.config.record {
            self.recording = Some(Recording {
                seed,
                limit,
                shared: Some(self.shared),
                start: start.iter().map(Career::to_json).collect(),
                entries: Vec::new(),
            });
        }
        for (slot, p) in players.iter_mut().enumerate() {
            p.input = Input::default();
            p.known = Known::default();
            p.pending.clear();
            p.ready = false;
            if let Some(c) = p.conn.and_then(|id| self.conns.get_mut(&id)) {
                c.state = ConnState::Playing { pilot: slot };
            }
        }
        self.players = players;
        self.log.push(format!(
            "shift {} started with {} pilot(s)",
            self.shift_no,
            self.players.len()
        ));
        let from_tick = self.tick;
        for slot in 0..self.players.len() {
            if let Some(id) = self.players[slot].conn {
                let msg = ServerMsg::Welcome {
                    pilot: slot as u8,
                    from_tick,
                };
                self.send(id, channel::RELIABLE, &msg);
            }
        }
        self.send_lobby();
    }

    /// Connections opening and closing.
    fn take_events(&mut self) {
        while let Some(e) = self.renet.get_event() {
            match e {
                ServerEvent::ClientConnected { client_id } => {
                    self.conns.insert(
                        client_id,
                        Conn {
                            state: ConnState::Joining,
                            since: self.tick,
                            strikes: 0,
                            inputs: 0,
                            actions: 0,
                            chats: 0,
                        },
                    );
                }
                ServerEvent::ClientDisconnected { client_id, reason } => {
                    let gone = self.conns.remove(&client_id).map(|c| c.state);
                    if let Some(ConnState::Waiting) = gone {
                        // What they bought this second is saved before
                        // their copy in memory is dropped.
                        self.save_changed();
                        self.waiting.retain(|p| p.conn != Some(client_id));
                        self.log
                            .push(format!("client {client_id} left the lobby ({reason})"));
                    }
                    if let Some(ConnState::Playing { pilot }) = gone {
                        let p = &mut self.players[pilot];
                        p.conn = None;
                        let name = p.name.clone();
                        // A tug whose player left waits out of the field,
                        // with nothing held when it returns.
                        self.release_controls(pilot);
                        self.world.set_away(pilot);
                        self.record(Entry::Away { pilot });
                        self.log.push(format!("{name} left ({reason})"));
                    }
                }
            }
        }
    }

    /// Lets go of everything pilot `pilot`'s connection was holding, when
    /// that connection ends or is replaced: the controls queued and held,
    /// and its message numbers.
    fn release_controls(&mut self, pilot: usize) {
        let p = &mut self.players[pilot];
        p.queue.clear();
        // Message numbers belong to the connection: a returning game opens
        // a new one and counts from 1 again, so the old numbers would drop
        // all of it.
        p.input_seq = 0;
        p.queued_seq = 0;
        p.action_seq = 0;
        if p.input != Input::default() {
            p.input = Input::default();
            self.record(Entry::Input {
                pilot,
                input: Input::default(),
            });
        }
    }

    /// Drops connections that never said Hello, and refused ones whose
    /// reason has had time to arrive.
    fn expire_conns(&mut self) {
        let tick = self.tick;
        let done: Vec<ClientId> = self
            .conns
            .iter()
            .filter(|(_, c)| match c.state {
                ConnState::Joining => tick - c.since > HELLO_TIMEOUT,
                ConnState::Leaving { at } => tick >= at,
                ConnState::Playing { .. } | ConnState::Waiting => false,
            })
            .map(|(id, _)| *id)
            .collect();
        for id in done {
            if matches!(self.conns[&id].state, ConnState::Joining) {
                self.log
                    .push(format!("client {id} dropped: no Hello within 5 s"));
            }
            self.renet.disconnect(id);
        }
    }

    /// Counts an invalid message against client `id`; too many and it goes.
    fn strike(&mut self, id: ClientId) {
        let c = self.conns.get_mut(&id).expect("connected");
        c.strikes += 1;
        if c.strikes >= MAX_STRIKES {
            self.log.push(format!(
                "client {id} disconnected: too many invalid messages"
            ));
            self.renet.disconnect(id);
        }
    }

    fn send(&mut self, id: ClientId, ch: u8, msg: &ServerMsg) {
        self.renet.send_message(id, ch, cj_net::encode(msg));
    }

    /// Refuses client `id` with `why`, then lets it go.
    fn reject(&mut self, id: ClientId, why: Reject) {
        self.log.push(format!("client {id} refused: {why:?}"));
        self.send(id, channel::RELIABLE, &ServerMsg::Reject(why));
        let at = self.tick + REJECT_LINGER;
        if let Some(c) = self.conns.get_mut(&id) {
            c.state = ConnState::Leaving { at };
        }
    }

    /// Everything client `id` sent since the last step.
    fn take_messages(&mut self, id: ClientId) {
        while let Some(bytes) = self.renet.receive_message(id, channel::RELIABLE) {
            if !self.conns.contains_key(&id) {
                return;
            }
            match cj_net::decode::<ClientMsg>(&bytes) {
                Ok(ClientMsg::Hello {
                    game_version,
                    player,
                    secret,
                    pilot,
                    view,
                }) => self.hello(id, game_version, player, secret, &pilot, view),
                Ok(ClientMsg::Action { seq, action }) => self.action(id, seq, action),
                Ok(ClientMsg::Query) => self.query(id),
                Ok(ClientMsg::Ready) => self.ready(id),
                Ok(ClientMsg::Chat { text }) => self.chat(id, &text),
                Ok(ClientMsg::Shop(req)) => self.shop(id, req),
                Ok(ClientMsg::Input { .. }) | Err(_) => self.strike(id),
            }
        }
        while let Some(bytes) = self.renet.receive_message(id, channel::UNRELIABLE) {
            if !self.conns.contains_key(&id) {
                return;
            }
            match cj_net::decode::<ClientMsg>(&bytes) {
                Ok(ClientMsg::Input { seq, inputs }) => self.input(id, seq, &inputs),
                _ => self.strike(id),
            }
        }
    }

    /// A Query before Hello: how full the server is. Each one also keeps
    /// the connection open (the player may be deciding whether to join);
    /// queries over the command rate limit are dropped.
    fn query(&mut self, id: ClientId) {
        let tick = self.tick;
        let c = self.conns.get_mut(&id).expect("connected");
        if !matches!(c.state, ConnState::Joining) {
            return self.strike(id);
        }
        c.actions += 1;
        if c.actions > MAX_ACTIONS_PER_SECOND {
            return;
        }
        c.since = tick;
        let connected = self
            .conns
            .values()
            .filter(|c| matches!(c.state, ConnState::Playing { .. } | ConnState::Waiting))
            .count();
        let info = Info {
            version: self.config.version.clone(),
            connected: connected as u8,
            max: self.config.max_players as u8,
            full: self.full(),
            pvp: self.config.pvp,
        };
        self.send(id, channel::RELIABLE, &ServerMsg::Info(info));
    }

    /// A public chat line from a player in the shift or the lobby: cleaned,
    /// rate-limited, and passed on to everyone connected under the name the
    /// server knows them by.
    fn chat(&mut self, id: ClientId, text: &str) {
        let Some(p) = player_of(&self.conns, &mut self.players, &mut self.waiting, id) else {
            return self.strike(id);
        };
        let from = p.name.clone();
        // The sender's slot colours the line; the lobby has none.
        let slot = self.pilot_of(id).map(|s| s as u8);
        let c = self.conns.get_mut(&id).expect("connected");
        c.chats += 1;
        let text = cj_net::clean_chat(text);
        if c.chats > MAX_CHATS_PER_SECOND || text.is_empty() {
            return;
        }
        let msg = ServerMsg::Chat(cj_net::ChatLine { from, slot, text });
        let to: Vec<ClientId> = self
            .players
            .iter()
            .chain(&self.waiting)
            .filter_map(|p| p.conn)
            .collect();
        for to in to {
            self.send(to, channel::RELIABLE, &msg);
        }
    }

    /// A shop request or belt vote in the break, checked against the
    /// player's career with the single-player shop rules, marked for saving
    /// when it goes through ([`Server::save_changed`]), and answered either
    /// way. Outside the break the shop is shut; requests over the command
    /// rate limit are dropped.
    fn shop(&mut self, id: ClientId, req: cj_net::ShopReq) {
        use cj_net::ShopReq as R;
        let Some(p) = player_of(&self.conns, &mut self.players, &mut self.waiting, id) else {
            return self.strike(id);
        };
        let c = self.conns.get_mut(&id).expect("connected");
        c.actions += 1;
        if c.actions > MAX_ACTIONS_PER_SECOND {
            return;
        }
        let mode = self.config.mode;
        let res: Result<String, String> = if !matches!(self.stage, Stage::Break { .. }) {
            Err("The shop opens between shifts.".to_owned())
        } else {
            let c = &mut p.career;
            match (req, req.apply(c)) {
                // A won career starts again fresh, ready for the next shift.
                (_, Ok((_, Some(Outcome::FleetPaid)))) => {
                    *c = Career::new(mode);
                    c.conditions = Some((c.shift, self.conditions));
                    c.ensure_offers(&mut self.rng);
                    Ok("The fleet is paid off. A fresh career starts next shift.".to_owned())
                }
                // A vote the player's tug can fly counts toward the next belt.
                (R::Vote(b), Ok(_)) => {
                    p.vote = Some(b);
                    Ok(format!("Your vote: {}.", b.def().name))
                }
                (_, res) => res.map(|(msg, _)| msg),
            }
        };
        let (ok, msg) = match res {
            Ok(m) => (true, m),
            Err(m) => (false, m),
        };
        p.unsaved |= ok;
        let reply = ServerMsg::Shop(cj_net::ShopReply {
            ok,
            msg,
            career: p.career.to_json(),
        });
        self.send(id, channel::RELIABLE, &reply);
    }

    /// A player is ready for the next shift. Outside the break it means
    /// nothing (it may simply have crossed the next shift's Welcome).
    fn ready(&mut self, id: ClientId) {
        if !self.in_break() {
            return;
        }
        match player_of(&self.conns, &mut self.players, &mut self.waiting, id) {
            Some(p) => p.ready = true,
            None => self.strike(id),
        }
    }

    /// The name a pilot is shown by: the one they sent, cleaned, or, when
    /// nothing is left of it (a name only in scripts the fonts can't draw),
    /// the first "Pilot N" nobody on the server has, so two nameless players
    /// can still be told apart in the lobby list and the chat. The caller
    /// clears a returning player's old name first, so it doesn't count.
    fn pilot_name(&self, raw: &str) -> String {
        let name = cj_net::clean_name(raw);
        if !name.is_empty() {
            return name;
        }
        let taken = |d: &str| {
            self.players
                .iter()
                .chain(&self.waiting)
                .any(|p| p.name == d)
        };
        (1..)
            .map(|n| format!("Pilot {n}"))
            .find(|d| !taken(d))
            .expect("fewer players than numbers")
    }

    /// A client's Hello: check its version and identity, then give it a pilot.
    /// A newcomer near the shift's end, or in the break, waits in the lobby.
    fn hello(
        &mut self,
        id: ClientId,
        version: String,
        player: PlayerId,
        secret: [u8; 16],
        name: &str,
        view: (u16, u16),
    ) {
        if !matches!(self.conns[&id].state, ConnState::Joining) {
            return self.strike(id);
        }
        if version != self.config.version {
            let server = self.config.version.clone();
            return self.reject(id, Reject::VersionMismatch { server });
        }
        // The same player again, with the right secret, while the server
        // still holds a connection for them: the new connection takes over.
        // A game that crashed or lost its network leaves its old connection
        // on the books until it times out, and turning the player away for
        // that long helps nobody. The old connection is told why and let go.
        if let Some(at) = self.waiting.iter().position(|p| p.id == player) {
            if self.waiting[at].secret != secret {
                return self.reject(id, Reject::WrongSecret);
            }
            // A lobby player is taken in afresh below, their career read
            // back from where it is saved, so save it first.
            self.save_changed();
            if let Some(old) = self.waiting.remove(at).conn {
                self.reject(old, Reject::AlreadyConnected);
            }
        }
        let known = self.players.iter().position(|p| p.id == player);
        // A player the server isn't holding (new, back after a shift away,
        // or back after a restart) must hold the secret their saved career
        // was claimed with. Checked only once there is room, so a full
        // server writes nothing for strangers. This runs before the career
        // is loaded or created, so a refusal never leaves a career unbound.
        // Past the minute's cap on newcomers (see NEW_ID_WINDOW), only
        // players who already have a key here are let in.
        if known.is_none() && !self.full() {
            let may_bind = self.new_ids < self.config.max_players;
            match self.careers.claim(&player, &secret, may_bind) {
                Ok(bound) => self.new_ids += usize::from(bound),
                Err(careers::Refusal::Busy) => return self.reject(id, Reject::Busy),
                Err(careers::Refusal::WrongSecret) => return self.reject(id, Reject::WrongSecret),
                Err(careers::Refusal::Storage(why)) => {
                    self.log.push(why);
                    return self.reject(id, Reject::ServerError);
                }
            }
        }
        let slot = match known {
            Some(s) if self.players[s].secret != secret => {
                return self.reject(id, Reject::WrongSecret)
            }
            Some(s) => {
                // Still connected (see above): the tug stays in the field
                // and changes hands with nothing held.
                if let Some(old) = self.players[s].conn.take() {
                    self.release_controls(s);
                    self.reject(old, Reject::AlreadyConnected);
                }
                self.world.set_back(s);
                self.record(Entry::Back { pilot: s });
                s
            }
            None if self.full() => return self.reject(id, Reject::Full),
            // A new tug in the last minute would arrive with nothing left
            // to do but fly home, so it waits for the next shift.
            None if self.in_break()
                || self.world.shift_over
                || self.world.time_left < LATE_JOIN_CUTOFF =>
            {
                let career = self.career_for(&player);
                let mut p = Player::new(player, secret, career);
                p.name = self.pilot_name(name);
                p.view = cj_net::clean_view(view);
                p.conn = Some(id);
                // Arriving during a break, they missed the career the lobby
                // was sent when it began, and the client opens its shop only
                // once it has one: prepare and send theirs now, as the break
                // does for everyone already waiting (and saved, as it saves
                // theirs, so coming back can't roll a fresh set).
                let shop = self.in_break().then(|| {
                    p.career.conditions = Some((p.career.shift, self.conditions));
                    p.career.ensure_offers(&mut self.rng);
                    if let Err(e) = self.careers.save(&p.id, &p.career) {
                        self.log.push(e);
                    }
                    ServerMsg::Shop(cj_net::ShopReply {
                        ok: true,
                        msg: String::new(),
                        career: p.career.to_json(),
                    })
                });
                self.log
                    .push(format!("{} is waiting for the next shift", p.name));
                self.waiting.push(p);
                self.conns.get_mut(&id).expect("connected").state = ConnState::Waiting;
                self.send(id, channel::RELIABLE, &ServerMsg::Waiting);
                if let Some(msg) = shop {
                    self.send(id, channel::RELIABLE, &msg);
                }
                return;
            }
            None => {
                let career = self.career_for(&player);
                let slot = self.world.join_career_pilot(&career);
                debug_assert_eq!(slot, self.players.len(), "a pilot per player");
                self.record(Entry::Join {
                    career: career.to_json(),
                });
                self.players.push(Player::new(player, secret, career));
                slot
            }
        };
        self.players[slot].name.clear();
        let shown = self.pilot_name(name);
        let p = &mut self.players[slot];
        p.name = shown;
        p.view = cj_net::clean_view(view);
        p.conn = Some(id);
        // A returning player has to be sent every outline again.
        p.known = Known::default();
        self.log
            .push(format!("{} joined as pilot {}", p.name, slot + 1));
        self.conns.get_mut(&id).expect("connected").state = ConnState::Playing { pilot: slot };
        let msg = ServerMsg::Welcome {
            pilot: slot as u8,
            from_tick: self.shift_start,
        };
        self.send(id, channel::RELIABLE, &msg);
        // Back during the break, the career sent when it began went to the
        // old connection, and the client opens its shop (and belt vote)
        // only once it has one. The break already prepared its offers.
        if self.in_break() {
            let shop = ServerMsg::Shop(cj_net::ShopReply {
                ok: true,
                msg: String::new(),
                career: self.players[slot].career.to_json(),
            });
            self.send(id, channel::RELIABLE, &shop);
        }
    }

    /// The pilot client `id` flies, if it has been welcomed.
    fn pilot_of(&self, id: ClientId) -> Option<usize> {
        match self.conns.get(&id)?.state {
            ConnState::Playing { pilot } => Some(pilot),
            _ => None,
        }
    }

    /// New held controls: the newest numbered message wins, within the rate
    /// limit; non-finite values are refused.
    fn input(&mut self, id: ClientId, seq: u32, inputs: &[Input]) {
        let Some(pilot) = self.pilot_of(id) else {
            // Controls from a player who has just gone to the lobby are
            // late, not hostile.
            if matches!(self.conns[&id].state, ConnState::Waiting) {
                return;
            }
            return self.strike(id);
        };
        if self.in_break() {
            return;
        }
        let c = self.conns.get_mut(&id).expect("connected");
        c.inputs += 1;
        if c.inputs > MAX_INPUTS_PER_SECOND {
            return;
        }
        let n = inputs.len();
        if n == 0 || n > cj_net::INPUTS_PER_MESSAGE || (seq as usize) < n {
            return self.strike(id);
        }
        let Some(clean) = inputs
            .iter()
            .map(|i| cj_net::clean_input(*i))
            .collect::<Option<Vec<Input>>>()
        else {
            return self.strike(id);
        };
        let p = &mut self.players[pilot];
        // Steps already queued or applied are repeats of what this message
        // carries for cover; only newer ones are added.
        for (k, input) in clean.into_iter().enumerate() {
            let s = seq - (n - 1 - k) as u32;
            if s > p.queued_seq.max(p.input_seq) {
                p.queue.push_back((s, input));
                p.queued_seq = s;
            }
        }
        while p.queue.len() > INPUT_QUEUE_MAX {
            p.queue.pop_front();
        }
    }

    /// Each pilot's controls for this step: the next queued step if there is
    /// one, else the last controls held a step longer (a packet is late).
    fn take_inputs(&mut self) {
        let mut changed = Vec::new();
        for (pilot, p) in self.players.iter_mut().enumerate() {
            if let Some((s, input)) = p.queue.pop_front() {
                p.input_seq = s;
                if p.input != input {
                    p.input = input;
                    changed.push((pilot, input));
                }
            }
        }
        for (pilot, input) in changed {
            self.record(Entry::Input { pilot, input });
        }
    }

    /// A cable command, applied before the next step; duplicates and
    /// commands over the rate limit are dropped.
    fn action(&mut self, id: ClientId, seq: u32, action: Action) {
        let Some(pilot) = self.pilot_of(id) else {
            return self.strike(id);
        };
        if self.in_break() {
            return;
        }
        let c = self.conns.get_mut(&id).expect("connected");
        c.actions += 1;
        if c.actions > MAX_ACTIONS_PER_SECOND || seq <= self.players[pilot].action_seq {
            return;
        }
        self.players[pilot].action_seq = seq;
        self.world.action_for(pilot, action);
        self.record(Entry::Action { pilot, action });
    }

    /// Hands this step's events to the players who may hear them: everyone's
    /// events, each player's own, and the sound effects of other players
    /// within hearing (`cj_net::HEARING`). Radio lines and the like stay
    /// private.
    fn share_events(&mut self) {
        let events = std::mem::take(&mut self.world.events);
        let tug = |q: usize| self.world.pilots[q].ship.b;
        for (slot, p) in self.players.iter_mut().enumerate() {
            if p.conn.is_none() {
                continue;
            }
            let me = tug(slot);
            let hears = |e: &cj_sim::events::Emitted| match e.to {
                None => true,
                Some(to) if to == slot => true,
                Some(to) => {
                    let other = tug(to);
                    matches!(e.event, cj_sim::events::Event::Sfx(_))
                        && cj_sim::js::dist(me.x, me.y, other.x, other.y) < cj_net::HEARING
                }
            };
            p.pending
                .extend(events.iter().filter(|e| hears(e)).cloned());
        }
    }

    /// The lobby list for everyone connected: who is flying and who is
    /// waiting, this shift's scores, and how the shift or break stands.
    fn send_lobby(&mut self) {
        let row = |p: &Player, slot: Option<usize>, w: &World| {
            let pilot = slot.map(|s| &w.pilots[s]);
            LobbyPlayer {
                name: p.name.clone(),
                slot: slot.map(|s| s as u8),
                banked: pilot.map_or(0, |pl| cj_sim::js::round(pl.banked) as i64),
                kills: pilot.map_or(0, |pl| pl.stats.jumpers + pl.stats.pvp_kills),
                deaths: pilot.map_or(0, |pl| pl.stats.deaths),
                away: p.conn.is_none(),
                ready: p.ready,
            }
        };
        let players: Vec<LobbyPlayer> = self
            .players
            .iter()
            .enumerate()
            .map(|(s, p)| row(p, Some(s), &self.world))
            .chain(self.waiting.iter().map(|p| row(p, None, &self.world)))
            .collect();
        let phase = match self.stage {
            Stage::Shift => Phase::Shift {
                time_left: self.world.time_left.max(0.0) as f32,
            },
            Stage::Break { until } => Phase::Break {
                seconds_left: until.saturating_sub(self.tick) as f32 / 60.0,
            },
        };
        let votes = tally(self.players.iter().chain(&self.waiting));
        let msg = ServerMsg::Lobby(Lobby {
            votes,
            tick: self.tick,
            shift_no: self.shift_no,
            phase,
            players,
            max: self.config.max_players as u8,
        });
        let to: Vec<ClientId> = self
            .players
            .iter()
            .chain(&self.waiting)
            .filter_map(|p| p.conn)
            .collect();
        for id in to {
            self.send(id, channel::UNRELIABLE, &msg);
        }
    }

    /// Each connected player's snapshot, new looks and pending events.
    fn send_views(&mut self) {
        for slot in 0..self.players.len() {
            let Some(id) = self.players[slot].conn else {
                continue;
            };
            let p = &mut self.players[slot];
            let (snap, looks) = Snapshot::of(
                &self.world,
                slot,
                p.view,
                self.tick,
                p.input_seq,
                &mut p.known,
            );
            let events = std::mem::take(&mut p.pending);
            self.send(
                id,
                channel::UNRELIABLE,
                &ServerMsg::Snapshot(Box::new(snap)),
            );
            if !looks.is_empty() {
                self.send(id, channel::RELIABLE, &ServerMsg::Looks(looks));
            }
            if !events.is_empty() {
                self.send(id, channel::RELIABLE, &ServerMsg::Events(events));
            }
        }
    }
}

/// Opens the UDP transport a server listens on: `ip` (every IPv4 interface,
/// or loopback only) and `port`, for up to `max_players` pilots. Renet's
/// unsecure netcode: traffic is not encrypted.
pub fn bind_udp(
    ip: Ipv4Addr,
    port: u16,
    max_players: usize,
) -> std::io::Result<renet_netcode::NetcodeServerTransport> {
    use renet_netcode::{NetcodeServerTransport, ServerAuthentication, ServerConfig};
    let socket = UdpSocket::bind((ip, port))?;
    let addr: SocketAddr = socket.local_addr()?;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    NetcodeServerTransport::new(
        ServerConfig {
            current_time: now,
            // Room for refused clients to hear why while the slots are full.
            max_clients: max_players * 2,
            protocol_id: cj_net::PROTOCOL_ID,
            public_addresses: vec![addr],
            authentication: ServerAuthentication::Unsecure,
        },
        socket,
    )
}

/// How far behind its schedule the run loop may fall and still catch up by
/// running the missed steps back to back (15 steps at 60 Hz). Further
/// behind, the missed time is skipped instead.
pub const MAX_CATCH_UP: Duration = Duration::from_millis(250);

/// Runs `server` over `transport` at 60 Hz, sleeping to absolute deadlines
/// so timing errors don't add up. A short delay is caught up; a stall
/// longer than [`MAX_CATCH_UP`] is skipped. After every step `each` sees the server
/// (to drain its log, say) and returns whether to go on. When it says stop,
/// every client is told the server is closing.
///
/// A panic in a step (a bug) ends the run the same way, so players are told
/// at once instead of waiting out a timeout, and comes back as `Err` with
/// what went wrong (also in the log).
pub fn run(
    transport: &mut renet_netcode::NetcodeServerTransport,
    server: &mut Server,
    mut each: impl FnMut(&mut Server) -> bool,
) -> Result<(), String> {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    let step = Duration::from_secs_f64(STEP);
    // The schedule: step `n` is due at `start + step * n`. Both are reset
    // when the loop falls too far behind (see MAX_CATCH_UP).
    let mut start = Instant::now();
    let mut n: u32 = 0;
    let outcome = loop {
        let stepped = catch_unwind(AssertUnwindSafe(|| {
            if let Err(e) = transport.update(step, &mut server.renet) {
                server.log.push(format!("transport: {e}"));
            }
            server.update(step);
            transport.send_packets(&mut server.renet);
            each(server)
        }));
        match stepped {
            Ok(true) => {}
            Ok(false) => break Ok(()),
            Err(cause) => {
                let why = cause
                    .downcast_ref::<&str>()
                    .map(|s| (*s).to_owned())
                    .or_else(|| cause.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "unknown error".to_owned());
                server
                    .log
                    .push(format!("server stopped after an error: {why}"));
                break Err(why);
            }
        }
        n += 1;
        let deadline = start + step * n;
        match deadline.checked_duration_since(Instant::now()) {
            Some(wait) => std::thread::sleep(wait),
            // After a long stall (the machine slept, the VM was paused) the
            // lost time is dropped: running every missed step back to back
            // would jump the shift clock and fly the tugs on stale controls.
            None if Instant::now() - deadline > MAX_CATCH_UP => {
                server.log.push(format!(
                    "fell {:?} behind; skipping ahead",
                    Instant::now() - deadline
                ));
                start = Instant::now();
                n = 0;
            }
            None if Instant::now() - deadline > step * 3 => {
                server.log.push(format!(
                    "step {n} overran by {:?}",
                    Instant::now() - deadline
                ));
            }
            None => {}
        }
    };
    // Even after a failed step, the last second of shopping can still be
    // saved and the connections closed.
    let _ = catch_unwind(AssertUnwindSafe(|| {
        server.save_changed();
        server.renet.disconnect_all();
        transport.send_packets(&mut server.renet);
        transport.disconnect_all(&mut server.renet);
    }));
    outcome
}
