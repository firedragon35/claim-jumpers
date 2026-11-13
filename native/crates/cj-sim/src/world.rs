//! The shift in progress (the browser's global `G`) and its fixed step.
//!
//! Update order follows step() in src/js/16-simulation-step.js.

use crate::career::{tier, Career, Mode, Rep, Tier};
use crate::consts::*;
use crate::entities::*;
use crate::events::{Emitted, Event, Radio, Sfx, Tint};
use crate::input::{Action, Input};
use crate::items::ItemType;
use crate::js;
use crate::mods::Mods;
use crate::rng::Rng;

/// Camera centre in world units, and zoom (screen px per world unit).
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Camera {
    pub x: f64,
    pub y: f64,
    pub zoom: f64,
}

/// Counters for the shift in progress, shown on the report and read by contracts.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Stats {
    /// Items banked this shift (stored cores count too).
    pub items: u32,
    /// Largest single delivery (bankings within DELIVERY_WINDOW of each other).
    pub best_run: f64,
    pub deaths: u32,
    /// Rivals scrapped, and the value of cargo they escaped with.
    pub jumpers: u32,
    pub lost: f64,
    /// Other players' tugs this pilot wrecked (PvP; no reward). Defaulted
    /// so stats written before PvP still load.
    #[serde(default)]
    pub pvp_kills: u32,
}

/// This shift's credited kills by rival hull.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Kills {
    pub skiff: u32,
    pub cutter: u32,
    pub brute: u32,
    pub gunship: u32,
    pub collector: u32,
    pub law: u32,
    pub miner: u32,
    pub drone: u32,
    pub rogue: u32,
    pub jaws: u32,
    pub boss: u32,
}

/// Who answers for damage being dealt: credited with kills, bounties and
/// tallies, and blamed with wanted stars and lost standing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Blame {
    /// Nothing says. In single-player that is the player, as in the browser
    /// (every kill not marked otherwise is yours); with several pilots it is
    /// nobody, so damage that no code path traced to a pilot credits no one.
    #[default]
    Unset,
    /// Nobody: a rogue sentry's shot, or a blast no player set off.
    Nobody,
    /// The pilot at this index into [`World::pilots`].
    Pilot(usize),
}

/// What a server settles for a whole shared shift; each pilot's career
/// supplies the rest (see [`World::for_careers`]).
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SharedShift {
    /// The belt the players voted for, and its conditions this shift.
    pub belt: crate::field::Belt,
    pub cond: crate::field::Condition,
    /// The server's difficulty mode, which sets the rivals' arrival rate.
    pub mode: Mode,
    /// Shifts started on the server, counting this one.
    pub shift_no: i32,
    /// The rival grudge carried over from the last shift (the belt's floor
    /// applies on top).
    pub grudge: f64,
    /// The host allows PvP on this server ([`World::pvp_allowed`]).
    /// Defaulted so recordings made before PvP still load.
    #[serde(default)]
    pub pvp: bool,
}

/// One player's tug, the cable it tows, and everything the shift keeps for
/// that player: loadout, bank, counters, standing with the law, and the
/// career the shift settles into.
///
/// A shift holds one pilot per player in [`World::pilots`]. Single-player is
/// the one-pilot case: `pilots[0]` is the player's tug, and the golden traces
/// pin its behaviour exactly.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Pilot {
    pub ship: Ship,
    /// Item ids from the tug outward: chain[0] hangs off the tug.
    pub chain: Vec<u32>,
    /// The tug's performance numbers from this pilot's loadout.
    pub mods: Mods,
    /// Difficulty multipliers from this pilot's career mode (Standard mode
    /// is 1 and 1).
    pub diff_damage: f64,
    pub diff_fees: f64,
    /// Credits banked this shift.
    pub banked: f64,
    pub stats: Stats,
    /// The delivery in progress: its running total and when it last grew.
    pub delivery_total: f64,
    pub delivery_t: f64,
    /// This shift's credited kills, the rival bounties paid into its
    /// earnings, and rounds fired.
    pub kills: Kills,
    pub bounties: f64,
    pub shots_fired: u32,
    /// Seconds the UNDER ATTACK warning stays up, and when its klaxon last
    /// sounded.
    pub under_attack_t: f64,
    pub(crate) attack_alarm_at: f64,
    /// The career being played, as it stood when the shift began (plus any
    /// loan credit from confiscations). Collectors and the law read it.
    pub career: Option<Career>,
    /// Wanted stars (0..=5), and seconds until the next one fades.
    pub wanted: u32,
    pub wanted_cool_t: f64,
    /// Collector triggers for this pilot: whether a thin wallet has been
    /// checked, and when a fat cable may next draw a collector.
    pub low_wallet_checked: bool,
    pub fat_cable_ready_at: f64,
    /// Whether the tug is inside a reactor radiation cloud.
    pub in_cloud: bool,
    /// Rival kills this shift by method.
    pub habit_kills: crate::career::Habits,
    /// The money side of the opposition, settled at the end of the shift.
    pub impounded: bool,
    pub impound_fee: f64,
    pub confiscated: f64,
    pub loan_credit: f64,
    pub fines: f64,
    pub fine_from_cargo: f64,
    pub arrests: u32,
    /// The co-pilot aboard and the accepted contract.
    pub crew: Option<crate::field::Crew>,
    /// Seconds until the fixer can patch the hull again.
    #[serde(skip)]
    pub fixer_t: f64,
    pub contract: Option<crate::field::Contract>,
    /// Banked pieces by type (for delivery contracts).
    pub delivered: Vec<(ItemType, u32)>,
    /// Rogue cores stored at the bay this shift.
    pub cores_banked: u32,
    /// Cargo has been cut loose mid-swing this shift (a training step).
    pub threw_cargo: bool,
    /// Standing changes earned this shift, applied when it ends.
    pub rep_delta: Rep,
    /// PvP: whether this pilot is Protected, arming or Enabled (see
    /// [`crate::perm`]); always Protected at the start of a shift.
    pub pvp: crate::perm::Pvp,
    /// Seconds left on the combat lock, during which PvP can't be turned off.
    pub combat_t: f64,
    /// PvP pilots who attacked this one and may be hurt back, each with the
    /// seconds its payback has left (pilot indices).
    pub payback: Vec<(usize, f64)>,
    /// The latest hit another player dealt this tug, and the shift time it
    /// landed: who to credit if the tug is wrecked in that step.
    pub last_pvp_hit: Option<(usize, f64)>,
    /// The player has left, but the tug stays until its combat lock runs
    /// out (see [`World::set_away`]). Never sent to clients.
    #[serde(skip)]
    pub leaving: bool,
    /// Set while this pilot's player is disconnected: the tug is out of the
    /// field (see [`World::set_away`]). Never sent to clients.
    #[serde(skip)]
    pub away: Option<Away>,
}

/// What an away pilot took out of the field, to put back on return.
#[derive(Clone, Debug)]
pub struct Away {
    /// The tow chain's item ids, emptied while away so nothing looks for
    /// cargo that isn't in the field.
    pub chain: Vec<u32>,
    /// The towed cargo, in chain order.
    pub cargo: Vec<Item>,
    /// Whether the tug was flying (a wreck keeps its respawn countdown).
    pub alive: bool,
}

impl Pilot {
    /// A fresh tug at the bay, built for `mods`, with nothing on the cable,
    /// nothing banked and a clean record.
    pub fn new(mods: Mods) -> Pilot {
        Pilot {
            ship: Ship::new(&mods),
            chain: Vec::new(),
            mods,
            diff_damage: 1.0,
            diff_fees: 1.0,
            banked: 0.0,
            stats: Stats::default(),
            delivery_total: 0.0,
            delivery_t: -99.0,
            kills: Kills::default(),
            bounties: 0.0,
            shots_fired: 0,
            under_attack_t: 0.0,
            attack_alarm_at: -99.0,
            career: None,
            wanted: 0,
            wanted_cool_t: WANTED_COOLDOWN,
            low_wallet_checked: false,
            fat_cable_ready_at: 0.0,
            in_cloud: false,
            habit_kills: Default::default(),
            fixer_t: 0.0,
            impounded: false,
            impound_fee: 0.0,
            confiscated: 0.0,
            loan_credit: 0.0,
            fines: 0.0,
            fine_from_cargo: 0.0,
            arrests: 0,
            crew: None,
            contract: None,
            delivered: Vec::new(),
            cores_banked: 0,
            threw_cargo: false,
            rep_delta: Rep::default(),
            pvp: crate::perm::Pvp::Protected,
            combat_t: 0.0,
            payback: Vec::new(),
            last_pvp_hit: None,
            leaving: false,
            away: None,
        }
    }
}

/// The whole simulation state for one shift: the pilots, field entities, opposition, timers and the event queue. Stepping it advances the shift deterministically from `rng`.
pub struct World {
    pub rng: Rng,
    /// Seconds simulated this shift.
    pub t: f64,
    pub time_left: f64,
    pub shift_no: i32,
    /// The belt's price multiplier (1 in the starting belt).
    pub belt_value: f64,
    /// The field's edges, the same on both axes: 0 and WORLD_W for a solo
    /// field, further out for a server sized for more pilots (see
    /// [`World::size_for`]); the carrier stays at the centre.
    pub edge_lo: f64,
    pub edge_hi: f64,
    /// The players' tugs, cables and per-player state; always at least one,
    /// and `pilots[0]` is the local player in single-player.
    pub pilots: Vec<Pilot>,
    pub items: Vec<Item>,
    pub rocks: Vec<Rock>,
    pub wrecks: Vec<Wreck>,
    pub jumpers: Vec<Jumper>,
    pub crabs: Vec<Crab>,
    /// Your cutter rounds, and rival bolts.
    pub bullets: Vec<Bullet>,
    pub ebullets: Vec<Bullet>,
    /// Shared id counter for items and jumpers.
    pub next_id: u32,
    /// The rival crews' grudge (jumpers destroyed); sets their stage.
    pub grudge: f64,
    /// Countdowns to the next rival arrival and field refills. A test
    /// field starts them at infinity so nothing arrives on its own.
    pub jumper_timer: f64,
    pub rock_timer: f64,
    pub wreck_timer: f64,
    /// Difficulty and belt multipliers on rival arrivals (Standard mode,
    /// Hollis Belt: 1, 0 and 1).
    pub diff_jumper_rate: f64,
    pub diff_jumper_cap: f64,
    pub belt_rivals: f64,
    pub drones: Vec<crate::opposition::Drone>,
    pub collectors: Vec<crate::opposition::Collector>,
    pub nets: Vec<crate::opposition::Net>,
    pub sheriffs: Vec<crate::opposition::Sheriff>,
    pub miners: Vec<crate::opposition::Miner>,
    pub missiles: Vec<crate::opposition::Missile>,
    pub sentries: Vec<crate::opposition::Sentry>,
    pub refinery: Option<crate::opposition::Refinery>,
    /// The countdown to the next law unit, and to the next patrol.
    pub reinforce_t: Option<f64>,
    pub patrol_t: f64,
    /// Collector arrivals: seconds into the shift, and the pilot each comes for.
    pub collector_queue: Vec<(f64, usize)>,
    pub refinery_t: f64,
    pub miner_announce_t: f64,
    /// The mode's first collector shift and net-tearing multiplier.
    pub collector_from: i32,
    pub net_tear: f64,
    /// Who answers for damage dealt right now; see [`Blame`].
    pub blame: Blame,
    /// Reactor radiation clouds.
    pub clouds: Vec<RadCloud>,
    /// Seconds until a new set of refinery charges arrives.
    pub charge_respawn_t: f64,
    /// The habit the crews have adapted to.
    pub rival_counter: Option<crate::career::Habit>,
    /// What the crews' war chest has bought, and the cargo they got away with this shift.
    pub crew_level: usize,
    pub rival_loot: f64,
    pub(crate) intel_told: bool,
    pub vault_opened: bool,
    /// The belt being worked and its condition this shift.
    pub belt: crate::field::Belt,
    pub cond: crate::field::Condition,
    /// The field event: when it fires, whether it has, and the ion storm.
    pub event_t: f64,
    pub event_done: bool,
    pub ion_t: f64,
    /// An escort contract's hauler arrives at this time.
    pub escort_at: Option<f64>,
    pub hauler: Option<crate::field::Hauler>,
    pub escort_done: bool,
    pub escort_failed: bool,
    pub hold_opened: bool,
    /// The boss crew taken down this shift, if any.
    pub boss_down: Option<&'static str>,
    /// The co-op partner tug, and P2's controls for the next step.
    pub p2: Option<crate::coop::Partner>,
    pub p2_input: crate::coop::P2Input,
    /// The daily challenge's field event, which outranks the belt's.
    pub forced_event: Option<crate::field::Event>,
    /// The guided first shift, while it runs.
    pub tut: Option<crate::tutorial::Tutorial>,
    /// Rival crews respect you: half as many hunters come.
    pub rivals_good: bool,
    pub cam: Camera,
    /// The pilot the camera follows and whose screen flashes and shakes:
    /// the local player (always 0 in single-player).
    pub view_pilot: usize,
    /// Viewport size in logical pixels; the camera's zoom depends on it.
    pub view: (f64, f64),
    /// Screen shake amplitude and white flash, both decaying each step.
    pub shake: f64,
    pub screen_flash: f64,
    warned_minute: bool,
    warned_ten: bool,
    /// The shift has ended: the clock reached zero or the tug was impounded.
    pub shift_over: bool,
    /// Effects, sounds and radio lines raised since the last drain, each
    /// with the pilot it concerns.
    pub events: Vec<Emitted>,
    /// The pilot whose events are being raised right now, if any (see
    /// [`World::with_hearer`]).
    pub hearer: Option<usize>,
    /// The host allows PvP on this server (see [`crate::perm`]). Off in
    /// single player and by default; a server sets it on each new shift.
    pub pvp_allowed: bool,
}

/// Runs one hostile's AI step. With its engines out (EMP) it may still turn
/// and fire, but its velocity stays the drift it had going in.
macro_rules! emp_step {
    ($list:expr, $i:expr, $dt:expr, $step:expr) => {{
        let i = $i;
        let drift = $list
            .get(i)
            .filter(|e| e.emp_t > 0.0)
            .map(|e| (e.b.vx, e.b.vy));
        $step;
        if let (Some((vx, vy)), Some(e)) = (drift, $list.get_mut(i)) {
            e.b.vx = vx;
            e.b.vy = vy;
            e.emp_t = (e.emp_t - $dt).max(0.0);
        }
    }};
}

impl World {
    /// An empty field with a fresh stock tug at the bay.
    pub fn empty(rng: Rng, view: (f64, f64)) -> World {
        let mods = Mods::stock();
        World {
            rng,
            t: 0.0,
            time_left: SHIFT_SECONDS,
            shift_no: 1,
            pilots: vec![Pilot::new(mods)],
            belt_value: 1.0,
            edge_lo: 0.0,
            edge_hi: WORLD_W,
            items: Vec::new(),
            rocks: Vec::new(),
            wrecks: Vec::new(),
            jumpers: Vec::new(),
            crabs: Vec::new(),
            bullets: Vec::new(),
            ebullets: Vec::new(),
            next_id: 1,
            grudge: 0.0,
            jumper_timer: f64::INFINITY,
            rock_timer: f64::INFINITY,
            wreck_timer: f64::INFINITY,
            diff_jumper_rate: 1.0,
            diff_jumper_cap: 0.0,
            belt_rivals: 1.0,
            rivals_good: false,
            drones: Vec::new(),
            collectors: Vec::new(),
            nets: Vec::new(),
            sheriffs: Vec::new(),
            miners: Vec::new(),
            missiles: Vec::new(),
            sentries: Vec::new(),
            refinery: None,
            reinforce_t: None,
            patrol_t: f64::INFINITY,
            collector_queue: Vec::new(),
            refinery_t: f64::INFINITY,
            miner_announce_t: 0.0,
            collector_from: 2,
            net_tear: 1.0,
            blame: Blame::Unset,
            clouds: Vec::new(),
            charge_respawn_t: crate::opposition::CHARGE_RESPAWN_TIME,
            rival_counter: None,
            crew_level: 0,
            rival_loot: 0.0,
            intel_told: false,
            vault_opened: false,
            belt: crate::field::Belt::Hollis,
            cond: crate::field::Condition::Normal,
            event_t: f64::INFINITY,
            event_done: true,
            ion_t: 0.0,
            escort_at: None,
            hauler: None,
            escort_done: false,
            escort_failed: false,
            hold_opened: false,
            boss_down: None,
            p2: None,
            p2_input: Default::default(),
            forced_event: None,
            tut: None,
            view_pilot: 0,
            cam: Camera {
                x: BAY_X,
                y: BAY_Y,
                zoom: 1.0,
            },
            view,
            shake: 0.0,
            screen_flash: 0.0,
            warned_minute: false,
            warned_ten: false,
            shift_over: false,
            events: Vec::new(),
            hearer: None,
            pvp_allowed: false,
        }
    }

    /// A new shift: the field is seeded and Marge opens the channel.
    pub fn new_shift(rng: Rng, view: (f64, f64)) -> World {
        World::new_shift_for(rng, view, 1)
    }

    /// A new shift on a field sized for up to `limit` pilots.
    pub fn new_shift_for(rng: Rng, view: (f64, f64), limit: usize) -> World {
        let mut w = World::empty(rng, view);
        w.size_for(limit);
        w.setup_opposition();
        w.start_shift();
        w.setup_miners();
        w
    }

    /// A shared shift on a server, sized for up to `limit` pilots: `shared`
    /// settles what belongs to the whole field, and each of `careers` gives
    /// pilot `i` its tug, difficulty, crew and contract. With no careers the
    /// field still has its one waiting tug, as `new_shift_for` does.
    pub fn for_careers(
        rng: Rng,
        view: (f64, f64),
        limit: usize,
        shared: &SharedShift,
        careers: &[Career],
    ) -> World {
        let mut w = World::empty(rng, view);
        w.size_for(limit);
        w.shift_no = shared.shift_no;
        let d = shared.mode.def();
        w.diff_jumper_rate = d.jumper_rate;
        w.diff_jumper_cap = d.jumper_cap;
        let bd = shared.belt.def();
        w.belt = shared.belt;
        w.belt_value = bd.value;
        w.belt_rivals = bd.rivals;
        w.cond = shared.cond;
        w.grudge = shared.grudge.max(bd.grudge_floor);
        w.pvp_allowed = shared.pvp;
        for c in careers {
            w.add_career_pilot(c);
        }
        // The rival crews' levels follow the best-funded career among them.
        w.crew_level = careers.iter().map(Career::crew_level).max().unwrap_or(0);
        w.event_t = w
            .rng
            .range(crate::field::EVENT_WINDOW.0, crate::field::EVENT_WINDOW.1);
        // The server's first shift is the quiet one: no field event.
        w.event_done = shared.shift_no == 1;
        w.setup_opposition();
        w.start_shift();
        w.setup_after_populate();
        w.setup_miners();
        w
    }

    /// Adds the next pilot, flying career `c`'s tug at its mode's difficulty
    /// with its crew and contract, parked a little apart from the others at
    /// the bay; returns its index. The field's first career takes over the
    /// world's own waiting tug.
    pub fn add_career_pilot(&mut self, c: &Career) -> usize {
        let p = if self.pilots[0].career.is_none() {
            0
        } else {
            let mut pl = Pilot::new(c.mods());
            pl.ship.b.y += 70.0 * self.pilots.len() as f64;
            self.pilots.push(pl);
            self.pilots.len() - 1
        };
        let pl = &mut self.pilots[p];
        pl.mods = c.mods();
        let at = pl.ship.b;
        pl.ship = Ship::new(&pl.mods);
        pl.ship.b.x = at.x;
        pl.ship.b.y = at.y;
        let d = c.mode.def();
        pl.diff_damage = d.damage;
        pl.diff_fees = d.fees;
        pl.career = Some(c.clone());
        self.pilot_from_career(p, c);
        p
    }

    /// A pilot flying career `c` joins a shift already under way (see
    /// [`World::add_career_pilot`]); returns its index. Their sentries launch
    /// now, since the shift's own launch ran before they arrived. The server
    /// and its replays both join through here, so they stay in step.
    pub fn join_career_pilot(&mut self, c: &Career) -> usize {
        let p = self.add_career_pilot(c);
        self.launch_sentries(p);
        p
    }

    /// The next shift of a career (newGame): its loadout, difficulty, shift
    /// number, carried-over rival grudge, and the field the career has picked.
    pub fn for_career(rng: Rng, view: (f64, f64), c: &Career) -> World {
        let mut w = World::empty(rng, view);
        w.pilots[0].mods = c.mods();
        w.pilots[0].ship = Ship::new(&w.pilots[0].mods);
        w.shift_no = c.shift;
        let d = c.mode.def();
        w.pilots[0].diff_damage = d.damage;
        w.pilots[0].diff_fees = d.fees;
        w.diff_jumper_rate = d.jumper_rate;
        w.diff_jumper_cap = d.jumper_cap;
        // Crews who hate you start the shift already annoyed.
        let bad = tier(c.rep.rivals) == Tier::Bad;
        w.grudge = c.grudge + if bad { 2.0 } else { 0.0 };
        w.rivals_good = tier(c.rep.rivals) == Tier::Good;
        w.pilots[0].career = Some(c.clone());
        w.rival_counter = c.habits.dominant();
        w.crew_level = c.crew_level();
        w.setup_career_field(0, c);
        w.setup_opposition();
        w.start_shift();
        w.setup_after_populate();
        w.setup_miners();
        w
    }

    /// Arm the spawn timers, seed the field and open the radio channel.
    fn start_shift(&mut self) {
        let w = self;
        w.jumper_timer = first_jumper_delay(w.shift_no);
        w.rock_timer = ROCK_REFILL_TIME;
        w.wreck_timer = WRECK_REFILL_TIME;
        w.populate();
        w.radio(Radio::Start, true, false);
    }

    /// Queues an event for the front end (sounds, radio lines, popups) to
    /// drain, addressed to the pilot being processed (see `with_hearer`).
    pub(crate) fn emit(&mut self, e: Event) {
        self.events.push(Emitted {
            to: self.hearer,
            event: e,
            at: None,
        });
    }

    /// Run `f` with the events it raises addressed to pilot `p` (`None`:
    /// everyone).
    pub(crate) fn with_hearer<T>(
        &mut self,
        p: Option<usize>,
        f: impl FnOnce(&mut World) -> T,
    ) -> T {
        let was = self.hearer;
        self.hearer = p;
        let out = f(self);
        self.hearer = was;
        out
    }

    /// Whether `e` was raised since the last drain, for anyone.
    pub fn heard(&self, e: &Event) -> bool {
        self.events.iter().any(|n| &n.event == e)
    }
    /// Queues a sound effect event.
    pub(crate) fn sfx(&mut self, s: Sfx) {
        self.emit(Event::Sfx(s));
    }
    /// A sound from a place in the field, so the game can pan and muffle it.
    pub(crate) fn sfx_from(&mut self, s: Sfx, x: f64, y: f64) {
        self.events.push(Emitted {
            to: self.hearer,
            event: Event::Sfx(s),
            at: Some((x as f32, y as f32)),
        });
    }
    /// Queues a stock radio line by key; `force` interrupts the radio's cooldown and `once` plays it at most once per shift.
    pub(crate) fn radio(&mut self, key: Radio, force: bool, once: bool) {
        self.emit(Event::Radio {
            key,
            force,
            once,
            text: None,
        });
    }
    /// A radio line with its own text, shown and spoken as written.
    pub(crate) fn radio_text(&mut self, key: Radio, force: bool, once: bool, text: String) {
        self.emit(Event::Radio {
            key,
            force,
            once,
            text: Some(text),
        });
    }
    /// Queues a floating text popup at (`x`, `y`) in world coordinates.
    pub(crate) fn popup(&mut self, x: f64, y: f64, text: String, tint: Tint) {
        self.emit(Event::Popup { x, y, text, tint });
    }

    /// Create an item with a fresh id (not yet added to the field).
    pub fn new_item(&mut self, kind: ItemType, x: f64, y: f64, vx: f64, vy: f64) -> Item {
        let id = self.next_id;
        self.next_id += 1;
        let cond = self.cond.value_for(kind);
        make_item(&mut self.rng, id, kind, x, y, vx, vy, self.belt_value, cond)
    }

    /// Create a rival with a fresh id at the current grudge stage
    /// (not yet added to the field). `variant` None rolls one.
    pub fn new_jumper(&mut self, x: f64, y: f64, variant: Option<Variant>) -> Jumper {
        let id = self.next_id;
        self.next_id += 1;
        let stage = self.rival_stage();
        let hunter_mul = if self.rivals_good { 0.5 } else { 1.0 };
        let mut j = make_jumper(&mut self.rng, id, x, y, variant, stage, hunter_mul);
        if self.crew_level >= 1 {
            j.hp += CREW_HULL_BONUS;
        }
        if self.crew_level >= 2 {
            j.speed_mul *= CREW_SPEED_MUL;
        }
        j
    }

    /// Index of the item with this id in `items`, if it still exists.
    pub fn item_index(&self, id: u32) -> Option<usize> {
        self.items.iter().position(|it| it.id == id)
    }

    /// Index of the rival with this id in `jumpers`, if it still exists.
    pub fn jumper_index(&self, id: u32) -> Option<usize> {
        self.jumpers.iter().position(|j| j.id == id)
    }

    /// The rivals' grudge stage index (0 Business .. 3 Vendetta).
    pub fn rival_stage(&self) -> usize {
        rival_stage_index(self.grudge)
    }

    /// A point just outside a random field edge, at least `min_dist`
    /// from the tug when possible (edgePoint in 14-game-state.js).
    pub(crate) fn edge_point(&mut self, min_dist: f64) -> (f64, f64) {
        for _ in 0..20 {
            let p = match self.rng.int(0, 3) {
                0 => (
                    self.rng.range(self.edge_lo, self.edge_hi),
                    self.edge_lo - 40.0,
                ),
                1 => (
                    self.edge_hi + 40.0,
                    self.rng.range(self.edge_lo, self.edge_hi),
                ),
                2 => (
                    self.rng.range(self.edge_lo, self.edge_hi),
                    self.edge_hi + 40.0,
                ),
                _ => (
                    self.edge_lo - 40.0,
                    self.rng.range(self.edge_lo, self.edge_hi),
                ),
            };
            let clear = self.pilots.iter().all(|pl| {
                !pl.ship.alive || js::dist(p.0, p.1, pl.ship.b.x, pl.ship.b.y) > min_dist
            });
            if clear {
                return p;
            }
        }
        (-40.0, -40.0)
    }

    /// True when a point is clear of the carrier's safe zone and existing bodies.
    pub(crate) fn clear_spot(&self, x: f64, y: f64, r: f64, safe: f64) -> bool {
        if js::dist(x, y, CARRIER_X, CARRIER_Y) < safe {
            return false;
        }
        if self
            .rocks
            .iter()
            .any(|o| js::dist(o.b.x, o.b.y, x, y) < o.b.r + r + 20.0)
        {
            return false;
        }
        !self
            .wrecks
            .iter()
            .any(|o| js::dist(o.b.x, o.b.y, x, y) < o.b.r * 1.4 + r + 120.0)
    }

    /// Seed the field: wrecks first (they need space), then rocks, then
    /// starter salvage near the carrier so the first hook comes quickly.
    pub fn populate(&mut self) {
        let (mut n, mut tries) = (0, 0);
        let (wrecks, items) = (self.area_count(START_WRECKS), self.area_count(START_ITEMS));
        while n < wrecks && tries < 400 * self.area().ceil() as usize {
            tries += 1;
            let x = self.rng.range(self.edge_lo + 300.0, self.edge_hi - 300.0);
            let y = self.rng.range(self.edge_lo + 300.0, self.edge_hi - 300.0);
            if self.clear_spot(x, y, 160.0, 1100.0) {
                let w = make_wreck(&mut self.rng, x, y, self.shift_no);
                self.wrecks.push(w);
                n += 1;
            }
        }
        let (mut n, mut tries) = (0, 0);
        let rocks = self.area_count(START_ROCKS);
        while n < rocks && tries < 2000 * self.area().ceil() as usize {
            tries += 1;
            let roll = self.rng.next();
            let size = if roll < 0.4 {
                3
            } else if roll < 0.8 {
                2
            } else {
                1
            };
            let x = self.rng.range(self.edge_lo + 100.0, self.edge_hi - 100.0);
            let y = self.rng.range(self.edge_lo + 100.0, self.edge_hi - 100.0);
            if self.clear_spot(x, y, rock_size(size).0, 650.0) {
                let r = make_rock_drifting(&mut self.rng, size, x, y);
                self.rocks.push(r);
                n += 1;
            }
        }
        for _ in 0..items {
            let a = self.rng.angle();
            let d = self.rng.range(500.0, 1300.0);
            let kind = if self.rng.chance(0.6) {
                ItemType::Scrap
            } else {
                ItemType::Ore
            };
            let vx = self.rng.range(-10.0, 10.0);
            let vy = self.rng.range(-10.0, 10.0);
            let it = self.new_item(
                kind,
                CARRIER_X + a.cos() * d,
                CARRIER_Y + a.sin() * d,
                vx,
                vy,
            );
            self.items.push(it);
        }
    }

    /// A one-shot cable command for the first pilot (single-player).
    pub fn action(&mut self, a: Action) {
        self.action_for(0, a);
    }

    /// A one-shot cable command for pilot `p`. In the browser these run from
    /// key and button handlers between frames, i.e. before the next step.
    pub fn action_for(&mut self, p: usize, a: Action) {
        if self.shift_over {
            return;
        }
        match a {
            Action::Hook => self.with_hearer(Some(p), |w| w.try_hook(p)),
            Action::Drop => self.with_hearer(Some(p), |w| w.drop_last(p)),
            Action::Cut => self.with_hearer(Some(p), |w| w.cut_all(p)),
            Action::Grind => self.with_hearer(Some(p), |w| w.grind_last(p)),
            Action::TogglePvp => self.with_hearer(Some(p), |w| w.toggle_pvp(p)),
        }
    }

    /// Advance one fixed step of `dt` seconds (normally [`STEP`]) with the
    /// first pilot's controls (single-player).
    pub fn step(&mut self, input: &Input, dt: f64) {
        self.step_all(std::slice::from_ref(input), dt);
    }

    /// Advance one fixed step of `dt` seconds with each pilot's controls:
    /// `inputs[p]` flies `pilots[p]`.
    ///
    /// # Panics
    /// If `inputs` does not hold exactly one entry per pilot (a caller bug).
    pub fn step_all(&mut self, inputs: &[Input], dt: f64) {
        assert_eq!(inputs.len(), self.pilots.len(), "one input per pilot");
        if self.shift_over {
            return;
        }
        self.t += dt;
        self.time_left -= dt;

        for (p, input) in inputs.iter().enumerate() {
            self.with_hearer(Some(p), |w| w.update_ship(p, input, dt));
        }
        self.update_pvp(dt);
        self.update_partner(dt);
        self.update_scoop(dt);
        for j in 0..self.jumpers.len() {
            let b = self.jumpers[j].b;
            let p = self.nearest_pilot(b.x, b.y);
            emp_step!(
                self.jumpers,
                j,
                dt,
                self.with_hearer(Some(p), |w| w.update_jumper(j, dt))
            );
        }
        for i in 0..self.items.len() {
            let it = &mut self.items[i];
            it.angle += it.spin * dt;
            it.thrown_t = (it.thrown_t - dt).max(0.0);
            it.knock_t = (it.knock_t - dt).max(0.0);
            if it.fuse > 0.0 {
                it.fuse -= dt;
                if it.fuse <= 0.0 {
                    self.detonate(i);
                }
            }
        }
        for i in 0..self.rocks.len() {
            let r = &mut self.rocks[i];
            r.angle += r.spin * dt;
            r.flash = (r.flash - dt).max(0.0);
            // Temporary pebbles crumble to dust when their time is up.
            if let Some(c) = r.crumble_t.as_mut() {
                *c -= dt;
                if *c <= 0.0 && !r.dead {
                    r.dead = true;
                    let (x, y) = (r.b.x, r.b.y);
                    self.emit(Event::Burst {
                        x,
                        y,
                        n: 4,
                        tint: Tint::Rock,
                        speed: 30.0,
                    });
                }
            }
        }
        for w in &mut self.wrecks {
            w.angle += w.spin * dt;
            w.flash = (w.flash - dt).max(0.0);
        }

        self.integrate_all(dt);
        self.solve_cables(dt);
        self.resolve_collisions();
        self.update_bullets(dt);
        for c in 0..self.crabs.len() {
            let p = self.crabs[c].on;
            self.with_hearer(Some(p), |w| w.update_crab(c, dt));
        }
        self.update_refinery(dt);
        for i in 0..self.drones.len() {
            emp_step!(self.drones, i, dt, self.update_drone(i, dt));
        }
        for i in 0..self.collectors.len() {
            let p = self.collectors[i].target;
            emp_step!(
                self.collectors,
                i,
                dt,
                self.with_hearer(Some(p), |w| w.update_collector(i, dt))
            );
        }
        for i in 0..self.sheriffs.len() {
            let p = self.sheriffs[i].target;
            emp_step!(
                self.sheriffs,
                i,
                dt,
                self.with_hearer(Some(p), |w| w.update_sheriff(i, dt))
            );
        }
        for i in 0..self.miners.len() {
            let b = self.miners[i].b;
            let p = self.nearest_pilot(b.x, b.y);
            emp_step!(
                self.miners,
                i,
                dt,
                self.with_hearer(Some(p), |w| w.update_miner(i, dt))
            );
        }
        self.update_nets(dt);
        self.update_missiles(dt);
        for i in 0..self.sentries.len() {
            let p = self.sentries[i].pilot;
            self.with_hearer(Some(p), |w| w.update_sentry(i, dt));
        }
        // An impound ends the shift mid-step.
        if self.shift_over {
            return;
        }
        self.update_bay(dt);
        self.update_tutorial(dt);
        self.update_events(dt);
        self.update_wanted_cooldown(dt);
        self.update_rival_intel();
        self.update_opposition_spawns(dt);
        self.update_spawns(dt);
        self.shake = (self.shake - dt * 30.0).max(0.0);
        self.screen_flash = (self.screen_flash - dt * 1.6).max(0.0);
        for pl in &mut self.pilots {
            pl.under_attack_t = (pl.under_attack_t - dt).max(0.0);
        }
        for p in 0..self.pilots.len() {
            self.with_hearer(Some(p), |w| w.fixer_patch(p, dt));
        }
        self.sweep_dead();
        self.update_camera(dt);

        if !self.warned_minute && self.time_left <= 60.0 {
            self.warned_minute = true;
            self.radio(Radio::Minute, true, false);
        }
        if !self.warned_ten && self.time_left <= 10.0 {
            self.warned_ten = true;
            self.radio(Radio::TenSec, true, false);
        }
        if self.time_left <= 0.0 {
            self.shift_over = true;
        }
    }

    /// Behind the menus the field keeps drifting while the camera circles
    /// the carrier (stepAmbient). Nothing collides and no rules run.
    pub fn step_ambient(&mut self, dt: f64) {
        self.t += dt;
        for r in &mut self.rocks {
            r.b.x += r.b.vx * dt;
            r.b.y += r.b.vy * dt;
            r.angle += r.spin * dt;
        }
        for w in &mut self.wrecks {
            w.b.x += w.b.vx * dt;
            w.b.y += w.b.vy * dt;
            w.angle += w.spin * dt;
        }
        for it in &mut self.items {
            it.b.x += it.b.vx * dt;
            it.b.y += it.b.vy * dt;
            it.angle += it.spin * dt;
        }
        // updateEffects runs too: shake, flash and the attack warning fade.
        self.shake = (self.shake - dt * 30.0).max(0.0);
        self.screen_flash = (self.screen_flash - dt * 1.6).max(0.0);
        for pl in &mut self.pilots {
            pl.under_attack_t = (pl.under_attack_t - dt).max(0.0);
        }
        let a = self.t * 0.05;
        self.cam.x = CARRIER_X + a.cos() * 500.0;
        self.cam.y = CARRIER_Y + a.sin() * 350.0;
        self.cam.zoom = (self.view.0.min(self.view.1) / 1100.0).clamp(0.45, 0.9);
    }

    /// Pilot `p`'s tug flight: rotation and thrust, both blunted by the
    /// weight on the winch.
    pub(crate) fn update_ship(&mut self, p: usize, input: &Input, dt: f64) {
        // An away tug is frozen out of the field: no flight, no respawn. An
        // impounded one is gone for the rest of the shift.
        if self.pilots[p].away.is_some() || self.pilots[p].impounded {
            return;
        }
        let s = &mut self.pilots[p].ship;
        s.flash = (s.flash - dt).max(0.0);
        if !s.alive {
            s.respawn_t -= dt;
            if s.respawn_t <= 0.0 {
                self.respawn_ship(p);
            }
            self.emit(Event::Thrust {
                level: 0.0,
                load: 0.0,
                sputtering: false,
            });
            return;
        }
        s.invuln = (s.invuln - dt).max(0.0);
        let load = self.tow_mass(p);
        let load_ratio = load / self.pilots[p].mods.winch_cap;
        let pl = &mut self.pilots[p];
        let (mods, s) = (&pl.mods, &mut pl.ship);

        // Heat sink shared by the guns and side thrusters.
        s.heat = (s.heat - HEAT_COOL * dt).max(0.0);
        let mut overheat = false;
        if !s.overheated && s.heat >= 100.0 {
            s.overheated = true;
            s.heat = 100.0;
            overheat = true;
        }
        if s.overheated && s.heat < OVERHEAT_RESUME {
            s.overheated = false;
        }

        // Max turn rate and responsiveness both drop with load, so a heavy
        // train makes the tug lazy to start and slow to stop rotating.
        let turn_load = load + mods.turn_load;
        let turn_max = SHIP_TURN * mods.turn_mul / (1.0 + turn_load / 7.0);
        let turn_resp = 9.0 / (1.0 + turn_load / 6.0);
        s.av += (input.turn * turn_max - s.av) * (turn_resp * dt).min(1.0);

        // Netted: each hard swap of turn direction tears the net; thrust
        // and side thrusters wear it down too.
        let turn_sign = if input.turn.abs() > 0.5 {
            input.turn.signum()
        } else {
            0.0
        };
        let mut ripped = false;
        let mut torn = false;
        if let Some(net) = s.net.as_mut() {
            if turn_sign != 0.0 && s.last_turn_sign != 0.0 && turn_sign != s.last_turn_sign {
                net.hp -= crate::opposition::NET_TEAR_PER_REVERSAL * self.net_tear;
                ripped = true;
            }
            if input.thrust > 0.0 {
                net.hp -= crate::opposition::NET_TEAR_THRUST * dt;
            }
            if mods.thrusters && input.strafe.abs() > 0.2 {
                net.hp -= crate::opposition::NET_TEAR_STRAFE * input.strafe.abs() * dt;
            }
            torn = net.hp <= 0.0;
        }
        if turn_sign != 0.0 {
            s.last_turn_sign = turn_sign;
        }
        s.angle += s.av * dt;
        s.spin_meter = s.spin_meter * (-0.35 * dt).exp() + (s.av.abs() / turn_max) * dt;

        // Forward wins over brake; reverse thrust is deliberately weaker.
        let mut throttle = if input.thrust > 0.0 {
            input.thrust
        } else {
            -REVERSE_THROTTLE * input.reverse
        };
        let mut force = SHIP_THRUST * mods.thrust_mul;
        // Only a crab on this tug fouls its nozzle: online, a crab on another
        // pilot's hull must not slow the whole field.
        if self
            .crabs
            .iter()
            .any(|c| c.state == CrabState::Latched && c.on == p)
        {
            force *= CRAB_LATCHED_THRUST;
        }
        if s.net.is_some() {
            force *= crate::opposition::NETTED_THRUST; // the net fouls the main nozzle
        }
        // Stunned by a sheriff bolt: engines at 45% while the charge bleeds off.
        if s.stun_t > 0.0 {
            s.stun_t -= dt;
            force *= 0.45;
        }
        // Afterburner: a squeeze of both triggers (or Shift) fires a burst
        // at full throttle. Each burst heats the engine; chain them and it
        // overheats, leaving the tug crawling until it cools.
        use crate::opposition::{BURN_HEAT, BURN_THRUST, BURN_TIME, ENGINE_COOL};
        use crate::opposition::{ENGINE_OVERHEAT_THRUST, ENGINE_OVERHEAT_TIME};
        if s.burn_t <= 0.0 {
            s.engine_heat = (s.engine_heat - ENGINE_COOL * dt).max(0.0);
        }
        let mut cooked = false;
        let mut burst = false;
        if s.engine_over_t > 0.0 {
            s.engine_over_t -= dt;
            force *= ENGINE_OVERHEAT_THRUST;
        } else if input.burn && !s.prev_burn && s.burn_t <= 0.0 {
            s.engine_heat += BURN_HEAT;
            if s.engine_heat >= 100.0 {
                s.engine_heat = 100.0;
                s.engine_over_t = ENGINE_OVERHEAT_TIME;
                s.burn_t = 0.0;
                cooked = true;
            } else {
                s.burn_t = BURN_TIME;
                burst = true;
            }
        }
        s.prev_burn = input.burn;
        if s.burn_t > 0.0 {
            s.burn_t -= dt;
            force *= BURN_THRUST;
            throttle = 1.0;
        }

        // Engines sputter under heavy loads: random short cut-outs.
        if throttle > 0.0 && load_ratio > SPUTTER_LOAD {
            s.sputter_t -= dt;
            if s.sputter_t <= 0.0 {
                s.sputter_t = self.rng.range(0.06, 0.4);
                s.sputtering = self.rng.chance(SPUTTER_CHANCE);
            }
            if s.sputtering {
                force *= SPUTTER_FORCE;
            }
        } else {
            s.sputtering = false;
        }

        s.b.vx += (s.angle.cos() * force * throttle / s.b.mass) * dt;
        s.b.vy += (s.angle.sin() * force * throttle / s.b.mass) * dt;
        s.thrust_level = throttle.abs();
        s.throttle = throttle;
        let sputtering = s.sputtering;

        // Side thrusters push perpendicular to the heading (+ = starboard)
        // and dump heat into the shared sink.
        s.strafe = if mods.thrusters && !s.overheated {
            input.strafe
        } else {
            0.0
        };
        let mut puff = None;
        if s.strafe != 0.0 {
            let (px, py) = (-s.angle.sin(), s.angle.cos());
            s.b.vx += (px * STRAFE_FORCE * mods.strafe_mul * s.strafe / s.b.mass) * dt;
            s.b.vy += (py * STRAFE_FORCE * mods.strafe_mul * s.strafe / s.b.mass) * dt;
            s.heat += HEAT_STRAFE * mods.strafe_heat_mul * s.strafe.abs() * dt;
            puff = Some(Event::Strafe {
                x: s.b.x,
                y: s.b.y,
                vx: s.b.vx,
                vy: s.b.vy,
                px,
                py,
                side: -s.strafe.signum(),
            });
        }
        let level = s.thrust_level.max(s.strafe.abs() * 0.6);

        // The cannon. Grinder slugs fire instead of cutter rounds while any
        // are left: slow, heavy, and they ignore heat.
        s.fire_t -= dt;
        let mut shots = 0;
        let mut slug = false;
        if input.fire && s.fire_t <= 0.0 && s.slugs > 0 {
            let (nx, ny) = (s.angle.cos(), s.angle.sin());
            s.slugs -= 1;
            s.fire_t = SLUG_INTERVAL;
            self.bullets.push(Bullet {
                blame: Blame::Pilot(p),
                x: s.b.x + nx * MUZZLE,
                y: s.b.y + ny * MUZZLE,
                vx: s.b.vx + nx * SLUG_SPEED,
                vy: s.b.vy + ny * SLUG_SPEED,
                life: SLUG_LIFE,
                dmg: SLUG_DAMAGE,
                slug: true,
                ..Bullet::default()
            });
            slug = true;
        } else if input.fire && s.fire_t <= 0.0 && mods.guns() > 0 && !s.overheated {
            let (nx, ny) = (s.angle.cos(), s.angle.sin());
            s.fire_t = FIRE_INTERVAL * mods.fire_mul;
            // Every barrel fires at once, in parallel.
            for &off in &mods.barrels {
                let (ox, oy) = (-ny * off, nx * off);
                self.bullets.push(Bullet {
                    blame: Blame::Pilot(p),
                    x: s.b.x + nx * MUZZLE + ox,
                    y: s.b.y + ny * MUZZLE + oy,
                    vx: s.b.vx + nx * BULLET_SPEED,
                    vy: s.b.vy + ny * BULLET_SPEED,
                    life: BULLET_LIFE,
                    dmg: mods.cannon_dmg,
                    slug: false,
                    laser: mods.laser,
                    ..Bullet::default()
                });
            }
            // Cannon levels add a fan of angled shots from the muzzle, in pairs.
            for k in 1..=mods.cannon_spread / 2 {
                for side in [-1.0, 1.0] {
                    let a = s.angle + side * f64::from(k) * CANNON_SPREAD_ANGLE;
                    self.bullets.push(Bullet {
                        blame: Blame::Pilot(p),
                        x: s.b.x + nx * MUZZLE,
                        y: s.b.y + ny * MUZZLE,
                        vx: s.b.vx + a.cos() * BULLET_SPEED,
                        vy: s.b.vy + a.sin() * BULLET_SPEED,
                        life: BULLET_LIFE,
                        dmg: mods.cannon_dmg,
                        slug: false,
                        laser: mods.laser,
                        ..Bullet::default()
                    });
                }
            }
            // Extra barrels add heat, but less than a full shot each, so a
            // three-barrel volley runs hot without being useless.
            s.heat += HEAT_PER_SHOT
                * (1.0 + 0.6 * (mods.guns() as f64 - 1.0) + 0.15 * f64::from(mods.cannon_spread))
                * mods.heat_mul;
            pl.shots_fired += 1;
            shots = mods.guns();
        }

        // Same 30 % line as the alarm below, so armour can't separate them.
        let low = !s.low_warned && s.hp < mods.max_hp * 0.3;
        if low {
            s.low_warned = true;
        }

        // Shield recharge: waits out the delay after the last hit, then
        // refills slowly (faster while parked at the bay).
        s.shield_flash = (s.shield_flash - dt).max(0.0);
        s.shield_delay_t = (s.shield_delay_t - dt).max(0.0);
        let mut shield_up = false;
        // An ion storm holds the shield at zero (see update_events).
        if s.shield_delay_t <= 0.0 && s.shield < mods.shield_max && self.ion_t <= 0.0 {
            let in_bay = js::dist(s.b.x, s.b.y, BAY_X, BAY_Y) < BAY_REPAIR_RANGE;
            let boost = if in_bay { SHIELD_BAY_BOOST } else { 1.0 };
            s.shield = mods
                .shield_max
                .min(s.shield + mods.shield_rate * boost * dt);
            shield_up = s.shield >= mods.shield_max;
        }

        // Looping alarms: low hull beeps, and a "wee-oo" warble while netted.
        s.alarm_t -= dt;
        let mut alarm = None;
        if s.alarm_t <= 0.0 {
            if s.net.is_some() {
                s.alarm_t = 1.0;
                alarm = Some(Sfx::NetAlarm);
            } else if s.hp < mods.max_hp * 0.3 {
                s.alarm_t = 0.9;
                alarm = Some(Sfx::Alarm);
            }
        }

        self.emit(Event::Thrust {
            level,
            load: load_ratio,
            sputtering,
        });
        if overheat {
            self.radio(Radio::Overheat, true, false);
            self.sfx(Sfx::Deny);
        }
        if let Some(e) = puff {
            self.emit(e);
        }
        if slug {
            self.sfx(Sfx::Slug);
        }
        if shots > 0 {
            self.sfx(Sfx::Shoot(1));
            // The double report of a double barrel.
            if shots > 1 {
                self.sfx(Sfx::Shoot(-1));
            }
        }
        if let Some(a) = alarm {
            self.sfx(a);
        }
        if ripped {
            let (x, y) = (self.pilots[p].ship.b.x, self.pilots[p].ship.b.y);
            self.burst(x, y, 4, Tint::Collector, 90.0);
            self.sfx(Sfx::Rip);
        }
        if cooked {
            self.sfx(Sfx::Alarm);
            self.radio(Radio::EngineOverheat, true, false);
        }
        if burst {
            self.sfx(Sfx::Missile);
        }
        if torn {
            self.free_ship(p, true);
        }
        if low {
            self.radio(Radio::HullLow, true, false);
        }
        if shield_up {
            self.sfx(Sfx::ShieldUp);
        }
    }

    /// Run `f` with any kill it causes left uncredited: no bounty, grudge,
    /// wanted stars or tally (withoutBlame).
    pub(crate) fn without_blame(&mut self, f: impl FnOnce(&mut World)) {
        self.with_blame(Blame::Nobody, f);
    }

    /// Run `f` with `blame` answering for the damage it deals. `Unset` leaves
    /// whatever blame is already in force.
    pub(crate) fn with_blame(&mut self, blame: Blame, f: impl FnOnce(&mut World)) {
        if blame == Blame::Unset {
            f(self);
            return;
        }
        let was = self.blame;
        self.blame = blame;
        f(self);
        self.blame = was;
    }

    /// The pilot a carried blame (on a round, missile or reactor) names:
    /// `Unset` defers to the blame in force now.
    pub fn resolve_blame(&self, blame: Blame) -> Option<usize> {
        match blame {
            Blame::Pilot(p) => Some(p),
            Blame::Nobody => None,
            Blame::Unset => self.blamed(),
        }
    }

    /// How much busier the field is than a solo one: 1 for one pilot, plus
    /// [`CROWD_SHARE`] for each extra pilot. Salvage, refills and rival
    /// arrivals scale by it so a crowd doesn't split one player's field.
    pub fn crowd(&self) -> f64 {
        1.0 + CROWD_SHARE * (self.pilots.len() as f64 - 1.0)
    }

    /// Sizes the field for up to `limit` pilots, before it is seeded: its
    /// area is a solo field's times the crowd factor for that many (1 for
    /// one pilot, plus CROWD_SHARE for each more; 20 pilots: 10.5x the area,
    /// about 3.2x as wide), grown evenly around the carrier.
    pub fn size_for(&mut self, limit: usize) {
        let crowd = 1.0 + CROWD_SHARE * (limit.max(1) as f64 - 1.0);
        let grow = (WORLD_W * crowd.sqrt() - WORLD_W) / 2.0;
        self.edge_lo = -grow;
        self.edge_hi = WORLD_W + grow;
    }

    /// The field's area as a multiple of a solo field's (1 when solo).
    pub fn area(&self) -> f64 {
        ((self.edge_hi - self.edge_lo) / WORLD_W).powi(2)
    }

    /// How many of a solo field's `n` a field this size holds (`n` when solo).
    pub(crate) fn area_count(&self, n: usize) -> usize {
        (n as f64 * self.area()).round() as usize
    }

    /// The local player's pilot: the one the camera follows ([`World::view_pilot`]).
    pub fn me(&self) -> &Pilot {
        &self.pilots[self.view_pilot]
    }

    /// The local player's pilot, to change.
    pub fn me_mut(&mut self) -> &mut Pilot {
        let p = self.view_pilot;
        &mut self.pilots[p]
    }

    /// The pilot whose tug is nearest (x, y), preferring live tugs; the
    /// first pilot wins a tie. With one pilot this is always 0.
    pub fn nearest_pilot(&self, x: f64, y: f64) -> usize {
        let d = |p: usize| js::dist(x, y, self.pilots[p].ship.b.x, self.pilots[p].ship.b.y);
        (0..self.pilots.len())
            .min_by(|&a, &b| {
                let (la, lb) = (self.pilots[a].ship.alive, self.pilots[b].ship.alive);
                lb.cmp(&la).then(d(a).total_cmp(&d(b)))
            })
            .unwrap_or(0)
    }

    /// Takes pilot `p`'s tug and towed cargo out of the field while its
    /// player is away, so nothing can hurt, rob or chase it; [`World::set_back`]
    /// puts both back where they were. The tug counts as not flying, which
    /// is what keeps it out of every system's reach.
    ///
    /// A tug under a PvP combat lock stays in the field, controls released,
    /// until the lock runs out, so quitting can't end a fight (owner,
    /// 2026-11-11); [`crate::perm`]'s clock takes it out then.
    pub fn set_away(&mut self, p: usize) {
        if self.pilots[p].away.is_some() {
            return;
        }
        if self.pilots[p].combat_t > 0.0 {
            self.pilots[p].leaving = true;
            return;
        }
        self.pilots[p].leaving = false;
        let chain = std::mem::take(&mut self.pilots[p].chain);
        let mut cargo = Vec::with_capacity(chain.len());
        for &id in &chain {
            if let Some(i) = self.items.iter().position(|it| it.id == id) {
                cargo.push(self.items.remove(i));
            }
        }
        let pl = &mut self.pilots[p];
        pl.away = Some(Away {
            chain,
            cargo,
            alive: pl.ship.alive,
        });
        pl.ship.alive = false;
    }

    /// Brings pilot `p`'s tug and cargo back into the field (see
    /// [`World::set_away`]).
    pub fn set_back(&mut self, p: usize) {
        // Back before a locked tug had left: it simply stays.
        self.pilots[p].leaving = false;
        let Some(away) = self.pilots[p].away.take() else {
            return;
        };
        self.pilots[p].ship.alive = away.alive;
        self.pilots[p].chain = away.chain;
        self.items.extend(away.cargo);
    }

    /// The pilot credited or blamed for damage dealt right now, if any.
    pub fn blamed(&self) -> Option<usize> {
        match self.blame {
            Blame::Pilot(p) => Some(p),
            Blame::Nobody => None,
            Blame::Unset => (self.pilots.len() == 1).then_some(0),
        }
    }

    /// All hull damage to pilot `p`'s tug funnels through here, so the
    /// difficulty scale applies once.
    pub fn damage_ship(&mut self, p: usize, n: f64) {
        self.with_hearer(Some(p), |w| w.hurt_tug(p, n));
    }

    /// The body of [`World::damage_ship`].
    fn hurt_tug(&mut self, p: usize, n: f64) {
        let pl = &mut self.pilots[p];
        let s = &mut pl.ship;
        if !s.alive || s.invuln > 0.0 || n <= 0.0 {
            return;
        }
        let mut n = n * pl.diff_damage;
        // The shield takes the hit first; any hit restarts the recharge delay.
        s.shield_delay_t = pl.mods.shield_delay;
        if s.shield > 0.0 {
            let absorbed = s.shield.min(n);
            s.shield -= absorbed;
            n -= absorbed;
            s.shield_flash = 0.25;
            let down = s.shield <= 0.0;
            if down {
                s.shield = 0.0;
            }
            if absorbed > 1.0 {
                self.sfx(Sfx::ShieldHit);
            }
            if down {
                self.sfx(Sfx::ShieldDown);
                self.radio(Radio::ShieldDown, true, false);
            }
            if n <= 0.0 {
                self.shake = (self.shake + absorbed * 0.15).min(14.0);
                return;
            }
        }
        let s = &mut self.pilots[p].ship;
        s.hp -= n;
        s.flash = 0.15;
        let dead = s.hp <= 0.0;
        self.shake = (self.shake + n * 0.5).min(14.0);
        if n > 2.0 {
            self.sfx(Sfx::Hurt);
            self.emit(Event::Rumble {
                strength: (n / 25.0).min(1.0),
                ms: 160.0,
            });
        }
        if dead {
            self.kill_ship(p);
        }
    }

    /// Wreck pilot `p`'s tug: cargo floats free, the deductible is charged,
    /// respawn soon.
    pub fn kill_ship(&mut self, p: usize) {
        self.with_hearer(Some(p), |w| w.wreck_tug(p));
    }

    /// The body of [`World::kill_ship`].
    fn wreck_tug(&mut self, p: usize) {
        let s = &mut self.pilots[p].ship;
        s.alive = false;
        s.hp = 0.0;
        s.respawn_t = RESPAWN_TIME;
        let (x, y) = (s.b.x, s.b.y);
        self.pilots[p].stats.deaths += 1;
        let pvp_wreck = self.credit_pvp_wreck(p);
        let deductible = js::round(DEDUCTIBLE * self.pilots[p].diff_fees * self.crew_fee_mul(p));
        self.pilots[p].banked = (self.pilots[p].banked - deductible).max(0.0);
        self.popup(
            x,
            y - 30.0,
            format!("-{deductible} deductible"),
            Tint::Danger,
        );
        if pvp_wreck {
            self.claim_spill(p);
        }
        self.sever_at(p, 0);
        for c in 0..self.crabs.len() {
            if self.crabs[c].state == CrabState::Latched && self.crabs[c].on == p {
                let vx = self.rng.range(-150.0, 150.0);
                let vy = self.rng.range(-150.0, 150.0);
                self.fling_crab(c, vx, vy);
            }
        }
        if self.pilots[p].ship.net.is_some() {
            self.free_ship(p, false);
        }
        self.clear_record(p);
        // A wrecked tug is a bad debt: a collector comes looking 20 s
        // later, once the mode's collectors have started appearing.
        let chasing = self.collectors.iter().any(|c| c.target == p);
        if !chasing && self.shift_no >= self.collector_from {
            self.collector_queue.push((self.t + 20.0, p));
        }
        self.explosion_fx(x, y, 150.0, Tint::Hazard);
        // The shockwave. Not the player's kills: dying near a sheriff earns no stars.
        self.without_blame(|w| w.explode(x, y, TUG_WRECK_BLAST_RADIUS, 0.0));
        self.shake = 18.0;
        self.sfx_from(Sfx::Boom, x, y);
        self.radio(Radio::Dead, true, false);
        // After the "tug is down" call, so Marge warns about them last.
        self.go_rogue(p);
    }

    /// Bring pilot `p`'s tug back at the bay with a fresh hull and a short
    /// grace period.
    fn respawn_ship(&mut self, p: usize) {
        let pl = &mut self.pilots[p];
        let s = &mut pl.ship;
        s.b.x = BAY_X;
        s.b.y = BAY_Y;
        s.b.vx = 0.0;
        s.b.vy = 0.0;
        s.angle = std::f64::consts::PI;
        s.av = 0.0;
        s.hp = pl.mods.max_hp;
        s.alive = true;
        s.invuln = SHIP_INVULN;
        s.low_warned = false;
        s.spin_meter = 0.0;
        s.shield = pl.mods.shield_max;
        s.shield_delay_t = 0.0;
        s.heat = 0.0;
        s.overheated = false;
        s.missiles = pl.mods.missile_rack;
        s.stun_t = 0.0;
        s.burn_t = 0.0;
        s.engine_heat = 0.0;
        s.engine_over_t = 0.0;
    }

    /// Any hostile opening fire on pilot `p`'s tug, or landing a hit, raises the
    /// UNDER ATTACK warning; the klaxon sounds at most every few seconds.
    pub(crate) fn under_attack(&mut self, p: usize) {
        if !self.pilots[p].ship.alive {
            return;
        }
        self.pilots[p].under_attack_t = ATTACK_HUD_HOLD;
        if self.t - self.pilots[p].attack_alarm_at > ATTACK_ALARM_GAP {
            self.pilots[p].attack_alarm_at = self.t;
            self.sfx(Sfx::AttackAlarm);
        }
    }

    /// The fireball's lasting effects on the screen (explosionFx); the
    /// particles themselves are drawn by the game from the event.
    pub(crate) fn explosion_fx(&mut self, x: f64, y: f64, size: f64, tint: Tint) {
        let k = size / 100.0;
        let s = &self.pilots[self.view_pilot].ship;
        // The closer you are, the harder the flash and shake hit.
        let near = if s.alive {
            (1.0 - js::dist(x, y, s.b.x, s.b.y) / 1500.0).clamp(0.0, 1.0)
        } else {
            0.3
        };
        self.screen_flash = self
            .screen_flash
            .max((0.18 * k).min(0.5) * (0.35 + 0.65 * near));
        self.shake = (self.shake + size * 0.07 * (0.3 + 0.7 * near)).min(26.0);
        self.emit(Event::Explosion { x, y, size, tint });
    }

    /// The bay banks cargo dropped inside its ring, tractors nearby pieces
    /// in, and repairs a tug parked close by.
    fn update_bay(&mut self, dt: f64) {
        for i in 0..self.items.len() {
            let it = &self.items[i];
            // Charges and keycards have no resale; the bay won't take them.
            if it.dead || matches!(it.kind, ItemType::Charge | ItemType::Keycard) {
                continue;
            }
            let d = js::dist(it.b.x, it.b.y, BAY_X, BAY_Y);
            if d < BAY_R - it.b.r * 0.3 {
                self.bank_item(i);
                continue;
            }
            // Bay tractor: cargo trailing a cable-length behind a parked tug
            // would otherwise hang just outside the ring.
            let reach = BAY_R * BAY_TRACTOR_REACH;
            if d < reach {
                let pull = BAY_TRACTOR_PULL * (1.0 - d / reach) * dt;
                let it = &mut self.items[i];
                it.b.vx += ((BAY_X - it.b.x) / d) * pull;
                it.b.vy += ((BAY_Y - it.b.y) / d) * pull;
            }
        }
        for p in 0..self.pilots.len() {
            self.with_hearer(Some(p), |w| w.serve_at_bay(p, dt));
        }
    }

    /// Pilot `p`'s tug parked at the bay: sanctuary from the law, repairs,
    /// and the crew prying crabs off the hull.
    fn serve_at_bay(&mut self, p: usize, dt: f64) {
        let s = &self.pilots[p].ship;
        if s.alive && js::dist(s.b.x, s.b.y, BAY_X, BAY_Y) < BAY_REPAIR_RANGE {
            // Sanctuary: the carrier is private property. Reaching the bay
            // wipes the wanted level and sends the law home.
            let chased = self.sheriffs.iter().any(|sh| {
                !sh.dead && sh.state == crate::opposition::SheriffState::Pursuit && sh.target == p
            });
            if self.pilots[p].wanted > 0 || chased {
                self.clear_record(p);
                self.radio(Radio::Sanctuary, true, false);
            }
            let max_hp = self.pilots[p].mods.max_hp;
            let s = &mut self.pilots[p].ship;
            let mut repaired_low = false;
            if s.hp < max_hp {
                s.hp = max_hp.min(s.hp + BAY_REPAIR_RATE * dt);
                repaired_low = s.hp < max_hp * 0.7;
            }
            if s.hp > max_hp * 0.4 {
                s.low_warned = false;
            }
            if repaired_low {
                self.radio(Radio::Repair, false, true);
            }
            // The bay crew pries crabs off the hull.
            for c in 0..self.crabs.len() {
                if self.crabs[c].state == CrabState::Latched && self.crabs[c].on == p {
                    self.kill_crab(c);
                }
            }
        }
    }

    /// Credit an item. Bankings within two seconds of each other count as
    /// one "delivery", which the end screen reports as the best delivery.
    pub fn bank_item(&mut self, i: usize) {
        // Cargo pays the pilot who hooked it; loose cargo drifting in pays
        // the pilot nearest the bay.
        let p = self.items[i]
            .owner
            .unwrap_or_else(|| self.nearest_pilot(BAY_X, BAY_Y));
        self.with_hearer(Some(p), |w| w.credit_item(p, i));
    }

    /// The body of [`World::bank_item`], crediting pilot `p`.
    fn credit_item(&mut self, p: usize, i: usize) {
        // Rogue cores are stored for the shop, not sold here.
        if self.items[i].kind == ItemType::Core {
            self.store_core(p, i);
            return;
        }
        // The broker gets a better price; the popup shows what was paid.
        if self.pilots[p].crew == Some(crate::field::Crew::Broker) {
            let it = &mut self.items[i];
            it.value = js::round(it.value * crate::field::BROKER_RATE);
        }
        let it = &mut self.items[i];
        let sling = !it.towed
            && it.thrown_t > 0.0
            && it.owner == Some(p)
            && it.b.vx.hypot(it.b.vy) >= SLING_SPEED;
        if sling {
            it.value = js::round(it.value * SLING_RATE);
            let (x, y) = (it.b.x, it.b.y);
            self.popup(x, y - 44.0, "SLINGSHOT".to_owned(), Tint::Cable);
            let line = SLING_LINES[self.rng.index(SLING_LINES.len())];
            self.radio_text(Radio::Bank, false, false, line.to_owned());
        }
        let it = &mut self.items[i];
        // Salvage from your own claim (or your own spilled PvP cargo) pays
        // a finder's premium.
        if it.claim.is_some_and(|(c, _)| c == p) {
            it.value = js::round(it.value * CLAIM_RATE);
        }
        if it.contraband() {
            it.value = js::round(it.value * CONTRABAND_RATE);
            let (x, y) = (it.b.x, it.b.y);
            self.popup(x, y - 60.0, "CONTRABAND".to_owned(), Tint::Danger);
        }
        let kind = self.items[i].kind;
        match self.pilots[p].delivered.iter_mut().find(|d| d.0 == kind) {
            Some(d) => d.1 += 1,
            None => self.pilots[p].delivered.push((kind, 1)),
        }
        let it = &self.items[i];
        let (x, y, value, rare, hidden) = (it.b.x, it.b.y, it.value, it.rare, it.hidden);
        let tint = it.tint;
        let named = rare || hidden || it.kind == ItemType::Blackbox;
        let label = it.label();
        self.pilots[p].banked += value;
        self.pilots[p].stats.items += 1;
        if self.t - self.pilots[p].delivery_t < DELIVERY_WINDOW {
            self.pilots[p].delivery_total += value;
        } else {
            self.pilots[p].delivery_total = value;
        }
        self.pilots[p].delivery_t = self.t;
        self.pilots[p].stats.best_run = self.pilots[p]
            .stats
            .best_run
            .max(self.pilots[p].delivery_total);
        self.emit(Event::Unlock(crate::events::Achievement::FirstBank));
        if self.pilots[p].delivery_total >= 5000.0 {
            self.emit(Event::Unlock(crate::events::Achievement::BigHaul));
        }
        if self.pilots[p].delivery_total >= 1500.0 {
            let text = format!("{} cr delivery", fmt_credits(self.pilots[p].delivery_total));
            self.emit(Event::Highlight {
                score: self.pilots[p].delivery_total,
                text,
            });
        }
        let text = if named {
            format!("+{} {}", fmt_credits(value), label.to_uppercase())
        } else {
            format!("+{}", fmt_credits(value))
        };
        let rare_tint = if rare {
            tint.map(Tint::Rgb).unwrap_or(Tint::Text)
        } else {
            Tint::Hazard
        };
        self.popup(x, y - 20.0, text, rare_tint);
        self.emit(Event::Burst {
            x,
            y,
            n: if named { 26 } else { 14 },
            tint: if rare {
                tint.map(Tint::Rgb).unwrap_or(Tint::Hazard)
            } else {
                Tint::Hazard
            },
            speed: if named { 200.0 } else { 140.0 },
        });
        let id = self.items[i].id;
        for c in 0..self.crabs.len() {
            if self.crabs[c].host == Some(id) {
                self.kill_crab(c);
            }
        }
        self.remove_item(i);
        self.sfx(if named { Sfx::Jackpot } else { Sfx::Bank });
        if hidden {
            let what = POD_CONTENTS[self.rng.index(POD_CONTENTS.len())];
            let v = fmt_credits(value);
            let text = format!("Pod cracked open. {what}. Bay says {v} cr. I've seen worse.");
            self.radio_text(Radio::Pod, true, false, text);
        } else if rare {
            let v = fmt_credits(value);
            let text = format!(
                "A {}. {v} cr. Well, well. I'm almost impressed. Almost.",
                label.to_lowercase()
            );
            self.radio_text(Radio::Rare, true, false, text);
        } else if value >= 800.0 {
            self.radio(Radio::BankBig, true, false);
        } else {
            self.radio(Radio::Bank, false, false);
        }
    }

    /// The fixer aboard pilot `p`'s tug patches a badly holed hull, then
    /// needs a minute before doing it again.
    fn fixer_patch(&mut self, p: usize, dt: f64) {
        let pl = &mut self.pilots[p];
        if pl.crew != Some(crate::field::Crew::Fixer) {
            return;
        }
        pl.fixer_t = (pl.fixer_t - dt).max(0.0);
        let max = pl.mods.max_hp;
        if pl.fixer_t > 0.0 || !pl.ship.alive || pl.ship.hp >= max * FIXER_PATCH_AT {
            return;
        }
        pl.fixer_t = FIXER_GAP;
        let s = &mut pl.ship;
        s.hp = (s.hp + FIXER_PATCH).min(max);
        let (x, y) = (s.b.x, s.b.y);
        self.popup(x, y - 40.0, "HULL PATCHED".to_owned(), Tint::Cable);
        self.sfx(Sfx::ShieldUp);
        let line = "Okafor says the hull's patched. He also says stop doing that.";
        self.radio_text(Radio::Repair, true, false, line.to_owned());
    }

    /// A hard knock to fragile cargo (a sealed pod or a flight recorder)
    /// breaks some of what's inside: it loses part of its value.
    pub(crate) fn knock_fragile(&mut self, i: usize) {
        let it = &mut self.items[i];
        if !matches!(it.kind, ItemType::Pod | ItemType::Blackbox) || it.knock_t > 0.0 {
            return;
        }
        it.knock_t = FRAGILE_GAP;
        let lost = js::round(it.value * FRAGILE_LOSS);
        it.value -= lost;
        let (x, y, hidden, towed) = (it.b.x, it.b.y, it.hidden, it.towed);
        // A sealed pod's worth is a secret until it's opened in the bay.
        let text = if hidden {
            "DAMAGED".to_owned()
        } else {
            format!("DAMAGED -{}", fmt_credits(lost))
        };
        self.popup(x, y - 24.0, text, Tint::Danger);
        if towed {
            let line = "Careful. Whatever's in that rattles now, and the bay pays for intact.";
            self.radio_text(Radio::Heavy, false, true, line.to_owned());
        }
    }

    /// Remove an item from the field, unhooking it from the chain.
    pub fn remove_item(&mut self, i: usize) {
        if self.items[i].dead {
            return;
        }
        let id = self.items[i].id;
        if self.items[i].towed {
            self.remove_from_chain(id);
        }
        // A rival dragging it loses its hook and goes hunting again.
        if let Some(jid) = self.items[i].hooked_by.take() {
            if let Some(j) = self.jumper_index(jid) {
                self.jumpers[j].hook = None;
                self.jumpers[j].state = JumperState::Hunt;
            }
        }
        let it = &mut self.items[i];
        it.dead = true;
        let (vx, vy) = (it.b.vx, it.b.vy);
        // Crabs riding or crawling off it drift free.
        for c in &mut self.crabs {
            if c.host == Some(id) && matches!(c.state, CrabState::Ride | CrabState::Crawl) {
                c.state = CrabState::Free;
                c.host = None;
                c.vx = vx;
                c.vy = vy;
            }
        }
    }

    fn sweep_dead(&mut self) {
        self.rocks.retain(|r| !r.dead);
        self.items.retain(|i| !i.dead);
        self.wrecks.retain(|w| !w.dead);
        // A dead rival lets go of its hook.
        for j in 0..self.jumpers.len() {
            if self.jumpers[j].dead {
                if let Some(id) = self.jumpers[j].hook.take() {
                    if let Some(i) = self.item_index(id) {
                        self.items[i].hooked_by = None;
                    }
                }
            }
        }
        self.jumpers.retain(|j| !j.dead);
        self.crabs.retain(|c| !c.dead);
        self.drones.retain(|d| !d.dead);
        self.collectors.retain(|c| !c.dead);
        self.miners.retain(|m| !m.dead);
        self.sheriffs.retain(|s| !s.dead);
        self.sentries.retain(|d| !d.dead);
        self.nets.retain(|n| n.life > 0.0);
        self.missiles.retain(|m| m.life > 0.0);
        let items = &self.items;
        for pl in &mut self.pilots {
            pl.chain.retain(|id| items.iter().any(|it| it.id == *id));
        }
    }

    /// The camera leads the tug's velocity and zooms out as the cargo train
    /// grows, so a long swinging load stays on screen.
    pub fn update_camera(&mut self, dt: f64) {
        let v = self.view_pilot;
        let s = &self.pilots[v].ship;
        let base = (self.view.0.min(self.view.1) / 900.0).clamp(0.5, 1.1);
        let span = self.pilots[v]
            .chain
            .iter()
            .filter_map(|id| self.items.iter().find(|it| it.id == *id))
            .map(|it| it.b.dist_to(&s.b))
            .fold(0.0, f64::max);
        let mut target_zoom = base * (1.0 - (span / 2200.0).min(0.32));
        let (mut tx, mut ty) = if s.alive {
            (s.b.x + s.b.vx * 0.35, s.b.y + s.b.vy * 0.35)
        } else {
            (self.cam.x, self.cam.y)
        };
        // Co-op: frame both tugs, zooming out as they spread apart. A
        // wrecked tug counts as back at the bay, where it will respawn.
        if let Some(p) = &self.p2 {
            let (ax, ay) = if s.alive {
                (s.b.x, s.b.y)
            } else {
                (BAY_X, BAY_Y)
            };
            tx = (ax + p.b.x) / 2.0;
            ty = (ay + p.b.y) / 2.0;
            target_zoom = target_zoom.min(base * 750.0 / js::dist(p.b.x, p.b.y, ax, ay).max(750.0));
        }
        let k = 1.0 - (-4.0 * dt).exp();
        self.cam.x += (tx - self.cam.x) * k;
        self.cam.y += (ty - self.cam.y) * k;
        self.cam.zoom += (target_zoom - self.cam.zoom) * (1.0 - (-2.0 * dt).exp());
    }

    /// Total mass on pilot `p`'s winch, tonnes.
    pub fn tow_mass(&self, p: usize) -> f64 {
        self.pilots[p]
            .chain
            .iter()
            .filter_map(|id| self.items.iter().find(|it| it.id == *id))
            .map(|it| it.b.mass)
            // Not .sum(): an empty f64 sum is -0.0, which shows as "-0 t".
            .fold(0.0, |a, b| a + b)
    }

    /// Total value of the cargo on pilot `p`'s line.
    pub fn line_value(&self, p: usize) -> f64 {
        self.pilots[p]
            .chain
            .iter()
            .filter_map(|id| self.items.iter().find(|it| it.id == *id))
            .map(|it| it.value)
            .fold(0.0, |a, b| a + b)
    }
}

/// Whole credits with thousands separators, like the browser's fmt().
pub fn fmt_credits(n: f64) -> String {
    let v = js::round(n) as i64;
    let digits = v.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if v < 0 {
        format!("-{out}")
    } else {
        out
    }
}
