//! The world as one pilot may see it, sent to that pilot's client.
//!
//! A snapshot carries every entity inside the area the client's camera can
//! show around its tug (plus a margin for what moves in before the next
//! snapshot) in full, and a radar dot for each radar-visible entity
//! further out. Nothing a player shouldn't know crosses the network: the
//! rng stays on the server, other careers are left out, wreck contents and
//! sealed pods' values are scrubbed, and on a night shift nothing beyond
//! the night radar's reach is sent at all.
//!
//! Rock and item outlines (and rock craters) never change, so each client
//! is sent them once, as [`Look`]s, the first time it sees that rock or
//! item; snapshots carry those entities with their outlines left empty.

use cj_sim::entities::{Bullet, Crab, Crater, Item, Jumper, RadCloud, Rock, ShapePt, Wreck};
use cj_sim::field::{Belt, Condition, Hauler, NIGHT_RADAR};
use cj_sim::opposition::{Collector, Drone, Miner, Missile, Net, Refinery, Sentry, Sheriff};
use cj_sim::world::{Pilot, World};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// World units sent beyond the camera's widest view on each side, so
/// entities that move into view between snapshots are already known.
const VIEW_MARGIN: f64 = 400.0;

/// How far the camera can zoom out from its base zoom (`World::update_camera`
/// zooms out by up to 32% for a long cargo train).
const CAMERA_FURTHEST: f64 = 1.0 - 0.32;

/// A rock's or item's fixed outline, sent once to each client that sees it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Look {
    Rock {
        id: u32,
        shape: Vec<ShapePt>,
        craters: Vec<Crater>,
    },
    Item {
        id: u32,
        shape: Vec<ShapePt>,
    },
}

/// The rocks and items whose looks one client already has. Rocks and
/// items number their ids separately.
#[derive(Clone, Debug, Default)]
pub struct Known {
    rocks: HashSet<u32>,
    items: HashSet<u32>,
}

/// What a radar dot stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlipKind {
    Item,
    Wreck,
    Jumper,
    Drone,
    Collector,
    Sheriff,
    Miner,
}

/// An entity beyond the camera, as the radar shows it: its kind and its
/// position rounded to whole world units (signed: a field sized for more
/// pilots reaches below zero).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Blip {
    pub kind: BlipKind,
    pub x: i32,
    pub y: i32,
}

/// Another pilot's tug in sight: its slot, the tug and its cable.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OtherTug {
    pub slot: u8,
    pub ship: cj_sim::entities::Ship,
    pub chain: Vec<u32>,
}

/// The world as pilot `you` may see it at server tick `tick`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub tick: u32,
    /// The newest input sequence number the server has applied for this
    /// client, so it can replay the inputs it sent since (prediction).
    pub ack: u32,
    /// This client's pilot, as an index into `pilots`.
    pub you: u8,
    // The shift's clock and the field's state.
    pub t: f64,
    pub time_left: f64,
    pub shift_over: bool,
    pub shift_no: i32,
    pub belt: Belt,
    pub cond: Condition,
    pub grudge: f64,
    pub ion_t: f64,
    pub reinforce_t: Option<f64>,
    pub event_t: f64,
    pub event_done: bool,
    /// The field's edges (see `World::edge_lo`).
    pub edge_lo: f64,
    pub edge_hi: f64,
    /// How many pilots the shift holds.
    pub pilot_count: u8,
    /// This client's own pilot in full, without its career.
    pub me: Pilot,
    /// Other pilots' tugs in sight.
    pub others: Vec<OtherTug>,
    // Everything in view, in full.
    pub items: Vec<Item>,
    pub rocks: Vec<Rock>,
    pub wrecks: Vec<Wreck>,
    pub jumpers: Vec<Jumper>,
    pub crabs: Vec<Crab>,
    pub bullets: Vec<Bullet>,
    pub ebullets: Vec<Bullet>,
    pub drones: Vec<Drone>,
    pub collectors: Vec<Collector>,
    pub nets: Vec<Net>,
    pub sheriffs: Vec<Sheriff>,
    pub miners: Vec<Miner>,
    pub missiles: Vec<Missile>,
    pub sentries: Vec<Sentry>,
    pub clouds: Vec<RadCloud>,
    /// The refinery and an escorted hauler are always sent: they are single,
    /// large, and on the radar wherever they are.
    pub refinery: Option<Refinery>,
    pub hauler: Option<Hauler>,
    /// Radar dots for what is out of view.
    pub blips: Vec<Blip>,
}

/// Which parts of the field pilot `you` may know about.
struct Sight {
    /// The centre: the pilot's tug.
    x: f64,
    y: f64,
    /// Half the width and height of the area sent in full.
    half: (f64, f64),
    /// On a night shift with the tug flying, nothing beyond this distance.
    reach: Option<f64>,
}

impl Sight {
    fn new(w: &World, you: usize, view: (f64, f64)) -> Sight {
        let s = &w.pilots[you].ship;
        // The camera's base zoom for this window (World::update_camera).
        let zoom = (view.0.min(view.1) / 900.0).clamp(0.5, 1.1) * CAMERA_FURTHEST;
        let night = w.cond.def().night && s.alive;
        Sight {
            x: s.b.x,
            y: s.b.y,
            half: (
                view.0 / 2.0 / zoom + VIEW_MARGIN,
                view.1 / 2.0 / zoom + VIEW_MARGIN,
            ),
            reach: night.then_some(NIGHT_RADAR),
        }
    }

    /// Within the night radar's reach (everywhere by day).
    fn in_reach(&self, x: f64, y: f64) -> bool {
        self.reach
            .is_none_or(|r| cj_sim::js::dist(x, y, self.x, self.y) < r)
    }

    /// Sent in full.
    fn sees(&self, x: f64, y: f64) -> bool {
        (x - self.x).abs() <= self.half.0
            && (y - self.y).abs() <= self.half.1
            && self.in_reach(x, y)
    }
}

/// The entities in `all` that `sight` sees, with radar dots for the rest
/// that `kind` says the radar shows.
fn split<T: Clone>(
    all: &[T],
    pos: impl Fn(&T) -> (f64, f64),
    kind: impl Fn(&T) -> Option<BlipKind>,
    sight: &Sight,
    blips: &mut Vec<Blip>,
) -> Vec<T> {
    let mut seen = Vec::new();
    for e in all {
        let (x, y) = pos(e);
        if sight.sees(x, y) {
            seen.push(e.clone());
        } else if let Some(kind) = kind(e).filter(|_| sight.in_reach(x, y)) {
            let q = |v: f64| v.round() as i32;
            blips.push(Blip {
                kind,
                x: q(x),
                y: q(y),
            });
        }
    }
    seen
}

impl Snapshot {
    /// The world as pilot `you`, with a window of `view` logical pixels,
    /// may see it, and the looks of anything in it this client (whose
    /// looks so far are `known`) hasn't been sent yet.
    pub fn of(
        w: &World,
        you: usize,
        view: (f64, f64),
        tick: u32,
        ack: u32,
        known: &mut Known,
    ) -> (Snapshot, Vec<Look>) {
        let sight = Sight::new(w, you, view);
        let mut blips = Vec::new();
        let b = |b: &cj_sim::entities::Body| (b.x, b.y);
        let mut items = split(
            &w.items,
            |e| b(&e.b),
            |it: &Item| it.worth_a_detour().then_some(BlipKind::Item),
            &sight,
            &mut blips,
        );
        // A sealed pod's value stays secret until it is opened.
        for it in items.iter_mut().filter(|it| it.hidden) {
            it.value = 0.0;
        }
        // Outlines travel once, as looks; the snapshot's copies go without.
        let mut looks = Vec::new();
        for it in &mut items {
            if known.items.insert(it.id) {
                looks.push(Look::Item {
                    id: it.id,
                    shape: std::mem::take(&mut it.shape),
                });
            } else {
                it.shape.clear();
            }
        }
        let mut rocks = split(&w.rocks, |e| b(&e.b), |_| None, &sight, &mut blips);
        for r in &mut rocks {
            let (shape, craters) = (std::mem::take(&mut r.shape), std::mem::take(&mut r.craters));
            if known.rocks.insert(r.id) {
                looks.push(Look::Rock {
                    id: r.id,
                    shape,
                    craters,
                });
            }
        }
        let mut wrecks = split(
            &w.wrecks,
            |e| b(&e.b),
            |_| Some(BlipKind::Wreck),
            &sight,
            &mut blips,
        );
        // What a hull holds (cargo and crabs) is found out by cutting it open.
        for wr in &mut wrecks {
            wr.contents.clear();
            wr.crabs = 0;
        }
        let mut me = w.pilots[you].clone();
        me.career = None;
        let others = w
            .pilots
            .iter()
            .enumerate()
            .filter(|(q, pl)| *q != you && sight.sees(pl.ship.b.x, pl.ship.b.y))
            .map(|(q, pl)| OtherTug {
                slot: q as u8,
                ship: pl.ship.clone(),
                chain: pl.chain.clone(),
            })
            .collect();
        let snapshot = Snapshot {
            tick,
            ack,
            you: you as u8,
            t: w.t,
            time_left: w.time_left,
            shift_over: w.shift_over,
            shift_no: w.shift_no,
            belt: w.belt,
            cond: w.cond,
            grudge: w.grudge,
            ion_t: w.ion_t,
            reinforce_t: w.reinforce_t,
            event_t: w.event_t,
            event_done: w.event_done,
            edge_lo: w.edge_lo,
            edge_hi: w.edge_hi,
            pilot_count: w.pilots.len() as u8,
            me,
            others,
            items,
            rocks,
            wrecks,
            jumpers: split(
                &w.jumpers,
                |e| b(&e.b),
                |_| Some(BlipKind::Jumper),
                &sight,
                &mut blips,
            ),
            crabs: split(&w.crabs, |e| (e.x, e.y), |_| None, &sight, &mut blips),
            bullets: split(&w.bullets, |e| (e.x, e.y), |_| None, &sight, &mut blips),
            ebullets: split(&w.ebullets, |e| (e.x, e.y), |_| None, &sight, &mut blips),
            drones: split(
                &w.drones,
                |e| b(&e.b),
                |_| Some(BlipKind::Drone),
                &sight,
                &mut blips,
            ),
            collectors: split(
                &w.collectors,
                |e| b(&e.b),
                |_| Some(BlipKind::Collector),
                &sight,
                &mut blips,
            ),
            nets: split(&w.nets, |e| (e.x, e.y), |_| None, &sight, &mut blips),
            sheriffs: split(
                &w.sheriffs,
                |e| b(&e.b),
                |_| Some(BlipKind::Sheriff),
                &sight,
                &mut blips,
            ),
            miners: split(
                &w.miners,
                |e| b(&e.b),
                |_| Some(BlipKind::Miner),
                &sight,
                &mut blips,
            ),
            missiles: split(&w.missiles, |e| (e.x, e.y), |_| None, &sight, &mut blips),
            sentries: split(&w.sentries, |e| (e.x, e.y), |_| None, &sight, &mut blips),
            clouds: split(&w.clouds, |e| (e.x, e.y), |_| None, &sight, &mut blips),
            refinery: w.refinery.clone(),
            hauler: w.hauler.clone(),
            blips,
        };
        (snapshot, looks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cj_sim::consts::STEP;
    use cj_sim::items::ItemType;
    use cj_sim::rng::Rng;
    use cj_sim::Input;

    const VIEW: (f64, f64) = (1280.0, 720.0);

    /// An empty field with the tug at (1000, 1000).
    fn field() -> World {
        let mut w = World::empty(Rng::Constant(0.5), VIEW);
        w.pilots[0].ship.b.x = 1000.0;
        w.pilots[0].ship.b.y = 1000.0;
        w
    }

    fn add(w: &mut World, x: f64, y: f64) -> usize {
        let it = w.new_item(ItemType::Scrap, x, y, 0.0, 0.0);
        w.items.push(it);
        w.items.len() - 1
    }

    /// Nearby items are sent in full; far ones only as radar dots, and only
    /// those the radar shows (a far piece of scrap sends nothing).
    #[test]
    fn far_entities_become_radar_dots() {
        let mut w = field();
        add(&mut w, 1100.0, 1000.0);
        let far = add(&mut w, 4800.0, 4800.0);
        w.items[far].rare = true;
        add(&mut w, 4000.0, 4000.0);
        let s = Snapshot::of(&w, 0, VIEW, 1, 0, &mut Known::default()).0;
        assert_eq!(s.items.len(), 1);
        assert_eq!(s.items[0].b.x, 1100.0);
        assert_eq!(
            s.blips,
            vec![Blip {
                kind: BlipKind::Item,
                x: 4800,
                y: 4800
            }]
        );
    }

    /// Hidden values and hull contents never leave the server.
    #[test]
    fn secrets_are_scrubbed() {
        let mut w = field();
        let i = add(&mut w, 1100.0, 1000.0);
        w.items[i].hidden = true;
        w.items[i].value = 900.0;
        let mut wr = cj_sim::entities::make_wreck(&mut w.rng, 1200.0, 1000.0, 3);
        wr.contents = vec![ItemType::Reactor];
        wr.crabs = 2;
        w.wrecks.push(wr);
        w.pilots[0].career = Some(cj_sim::career::Career::new(cj_sim::career::Mode::Standard));
        let s = Snapshot::of(&w, 0, VIEW, 1, 0, &mut Known::default()).0;
        assert_eq!(s.items[0].value, 0.0);
        assert!(s.wrecks[0].contents.is_empty());
        assert_eq!(s.wrecks[0].crabs, 0);
        assert!(s.me.career.is_none(), "careers stay on the server");
    }

    /// On a night shift nothing beyond the night radar is sent, not even a dot.
    #[test]
    fn night_hides_what_the_radar_cant_reach() {
        let mut w = field();
        w.cond = Condition::Night;
        assert!(w.cond.def().night);
        add(&mut w, 1000.0 + NIGHT_RADAR + 300.0, 1000.0);
        let s = Snapshot::of(&w, 0, VIEW, 1, 0, &mut Known::default()).0;
        assert!(s.items.is_empty() && s.blips.is_empty());
    }

    /// A snapshot survives encoding.
    #[test]
    fn snapshots_round_trip() {
        let mut w = World::new_shift(Rng::seeded(7), VIEW);
        w.step(&Input::default(), STEP);
        let s = Snapshot::of(&w, 0, VIEW, 5, 3, &mut Known::default()).0;
        // Entities don't implement PartialEq; equal bytes after a round trip
        // say the same thing.
        let bytes = crate::encode(&s);
        let back: Snapshot = crate::decode(&bytes).expect("decodes");
        assert_eq!(crate::encode(&back), bytes);
    }

    /// How big a snapshot of a busy shift is: reported, and bounded well
    /// under the message cap.
    #[test]
    fn a_busy_snapshot_stays_small() {
        let mut w = World::new_shift(Rng::seeded(7), VIEW);
        let mut largest = 0;
        let mut known = Known::default();
        for step in 0..(120.0 / STEP) as u32 {
            let input = Input {
                thrust: 0.7,
                turn: (f64::from(step) * STEP).sin() * 0.6,
                fire: true,
                ..Input::default()
            };
            if step % 120 == 0 {
                w.action(cj_sim::Action::Hook);
            }
            w.step(&input, STEP);
            w.events.clear();
            if step % 60 == 0 {
                let (snap, _) = Snapshot::of(&w, 0, VIEW, step, 0, &mut known);
                largest = largest.max(crate::encode(&snap).len());
            }
        }
        println!("largest snapshot: {largest} bytes");
        assert!(largest < 8 * 1024, "{largest} bytes");
    }

    /// A rock's or item's outline is sent once, the first time it's seen;
    /// later snapshots carry it without.
    #[test]
    fn outlines_are_sent_once() {
        let mut w = field();
        add(&mut w, 1100.0, 1000.0);
        let mut known = Known::default();
        let (first, looks) = Snapshot::of(&w, 0, VIEW, 1, 0, &mut known);
        assert!(first.items[0].shape.is_empty());
        assert!(matches!(&looks[..], [Look::Item { shape, .. }] if !shape.is_empty()));
        let (_, again) = Snapshot::of(&w, 0, VIEW, 2, 0, &mut known);
        assert!(again.is_empty(), "nothing new to send");
    }
}
