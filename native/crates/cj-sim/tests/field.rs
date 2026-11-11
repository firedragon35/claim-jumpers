//! The browser's own tests for the career's field systems
//! (tests/specs/phases-6-9.spec.js in the claim-jumpers-game repository), ported to the
//! Rust simulation: standing and truces, contract chains, belt conditions,
//! escorts, sealed holds and rogue cores. Each keeps the browser test's
//! setup and assertions.

use cj_sim::career::{Career, Mode};
use cj_sim::consts::*;
use cj_sim::entities::{make_crab, make_rock_drifting, Bullet, CrabState, RadCloud};
use cj_sim::field::*;
use cj_sim::items::ItemType;
use cj_sim::parts::{Part, System};
use cj_sim::rng::Rng;
use cj_sim::{Input, World};

const VIEW: (f64, f64) = (1280.0, 720.0);

/// startShift: a career shift with no rivals, collectors, law or field
/// event, and the tug parked well clear of the bay.
fn quiet(c: &Career, rng: Rng) -> World {
    let mut w = World::for_career(rng, VIEW, c);
    w.jumpers.clear();
    w.collectors.clear();
    w.sheriffs.clear();
    w.jumper_timer = 1e9;
    w.patrol_t = 1e9;
    w.collector_queue.clear();
    w.event_done = true;
    w.pilots[0].ship.b.x = BAY_X + 1400.0;
    w.pilots[0].ship.b.y = BAY_Y;
    w
}

fn step(w: &mut World) {
    w.step(&Input::default(), STEP);
}

#[test]
fn bank_standing_moves_installments_and_a_truce_drops_the_grudge_a_stage() {
    let mut c = Career::new(Mode::Standard);
    c.shift = 5;
    let neutral = c.due(0);
    c.rep.bank = 60.0;
    let good = c.due(0);
    c.rep.bank = -60.0;
    let bad = c.due(0);
    assert!(good < neutral, "{good} < {neutral}");
    assert!(bad > neutral, "{bad} > {neutral}");

    c.rep.bank = 0.0;
    c.wallet = 5000.0;
    c.grudge = 6.0;
    c.buy_truce().expect("affordable");
    // Hostile (5+) back to Annoyed (2).
    assert_eq!(c.grudge, 2.0);
    assert_eq!(c.rep.rivals, 10.0);
}

#[test]
fn a_contract_chain_advances_and_its_last_step_awards_a_unique_part() {
    let mut c = Career::new(Mode::Standard);
    c.shift = 6;
    c.chain = Some((Chain::SaltyDog, 2));
    c.contract = Some(chain_contract(Chain::SaltyDog, 2));
    c.contract_shift = 6;
    let mut w = quiet(&c, Rng::seeded(5));
    // The "clean" step: bank 3,500 without being wrecked.
    w.pilots[0].banked = 9000.0;
    c.settle(&w, 0);
    assert!(c.owns(Part::Grapple));
    assert_eq!(c.chain, None);
    assert_eq!(c.chains_done, vec![Chain::SaltyDog]);
}

#[test]
fn belt_conditions_change_sale_values() {
    let mut c = Career::new(Mode::Standard);
    c.shift = 3;
    let mut w = quiet(&c, Rng::seeded(9));
    let avg = |w: &mut World, cond: Condition| {
        w.cond = cond;
        let s: f64 = (0..2000)
            .map(|_| w.new_item(ItemType::Ore, 0.0, 0.0, 0.0, 0.0).value)
            .sum();
        s / 2000.0
    };
    let base = avg(&mut w, Condition::Normal);
    let boom = avg(&mut w, Condition::OreBoom);
    let r = boom / base;
    assert!(r > 1.4 && r < 1.6, "ore boom ratio {r}");
}

#[test]
fn escorting_a_hauler_home_meets_the_escort_contract() {
    let mut rng = Rng::seeded(4);
    let mut c = Career::new(Mode::Standard);
    c.shift = 4;
    c.contract = Some(make_contract(&mut rng, ContractKind::Escort, 4));
    c.contract_shift = 4;
    let mut w = quiet(&c, rng);
    for _ in 0..50 * 60 {
        if w.hauler.is_some() {
            break;
        }
        step(&mut w);
        w.pilots[0].ship.hp = 999.0;
    }
    let h = w.hauler.as_mut().expect("the hauler arrives on schedule");
    h.b.x = BAY_X + 300.0;
    h.b.y = BAY_Y;
    for _ in 0..600 {
        if w.escort_done {
            break;
        }
        step(&mut w);
    }
    let k = w.pilots[0].contract.clone().unwrap();
    assert!(w.escort_done);
    assert!(w.contract_progress(0, &k) >= k.n);
}

#[test]
fn a_sealed_hold_ignores_damage_and_opens_with_its_keycard() {
    let mut c = Career::new(Mode::Standard);
    c.shift = 3;
    let mut w = quiet(&c, Rng::seeded(21));
    for wr in &mut w.wrecks {
        wr.locked = false;
    }
    w.items.retain(|i| i.kind != ItemType::Keycard);
    w.setup_locked_hold();
    let wi = w
        .wrecks
        .iter()
        .position(|x| x.locked)
        .expect("a sealed hold");
    let ci = w
        .items
        .iter()
        .position(|i| i.kind == ItemType::Keycard)
        .expect("its keycard");

    let hp = w.wrecks[wi].hp;
    w.damage_wreck(wi, 99.0);
    assert_eq!(w.wrecks[wi].hp, hp, "sealed holds take no damage");
    assert!(!w.wrecks[wi].dead);

    // Throw the card at its wreck.
    let wb = w.wrecks[wi].b;
    let card = &mut w.items[ci].b;
    card.x = wb.x + wb.r * 1.1;
    card.y = wb.y;
    card.vx = (wb.x - card.x) * 2.0;
    card.vy = 0.0;
    let before = w.wrecks.len();
    for _ in 0..120 {
        if w.hold_opened {
            break;
        }
        step(&mut w);
    }
    // The opened hull breaks apart and is swept from the list (the browser
    // checks its dead flag through an object reference instead).
    assert!(w.hold_opened, "the hold opens");
    assert!(w.wrecks.iter().all(|x| !x.locked));
    assert!(w.wrecks.len() < before, "the hold broke apart");
    assert!(
        w.items.iter().all(|i| i.kind != ItemType::Keycard),
        "the card is used up"
    );
}

#[test]
fn a_rogue_sentry_drops_a_core_that_can_be_stored_and_reprogrammed() {
    let mut c = Career::new(Mode::Standard);
    c.shift = 3;
    c.owned.push(Part::DroneBay);
    c.mounted[3] = Some(Part::DroneBay);
    let mut w = quiet(&c, Rng::seeded(13));
    let mut d = w.make_sentry(0, 0, 1);
    d.rogue = true;
    d.wander = (0.0, 0.0);
    d.vx = 0.0;
    d.vy = 0.0;
    w.sentries.push(d);
    let n = w.sentries.len() - 1;
    w.hurt_sentry(n, 99.0);
    let core = w
        .items
        .iter()
        .position(|i| i.kind == ItemType::Core)
        .expect("the rogue leaves its core");
    w.bank_item(core);
    w.pilots[0].banked = 5000.0;
    c.settle(&w, 0);
    assert_eq!(c.cores, 1);
    c.reprogram_core().expect("a drone bay with room");
    assert_eq!((c.cores, c.sentries), (0, 1));
}

/// The Hollis Freight chain ends with the self-sealing hull: once
/// awarded it mounts, and it patches the hull at 1.5 a second in the field.
#[test]
fn the_awarded_self_sealing_hull_mounts_and_repairs_the_hull() {
    let mut c = Career::new(Mode::Standard);
    c.shift = 8;
    c.chain = Some((Chain::HollisFreight, 2));
    c.contract = Some(chain_contract(Chain::HollisFreight, 2));
    c.contract_shift = 8;
    let mut w = quiet(&c, Rng::seeded(3));
    w.vault_opened = true;
    w.pilots[0].banked = 4000.0;
    c.settle(&w, 0);
    assert!(c.owns(Part::HullSeal), "the chain's last step awards it");
    c.mount(Part::HullSeal).expect("an awarded part mounts");

    let mut w = quiet(&c, Rng::seeded(3));
    assert!(w.pilots[0].mods.hull_seal);
    w.pilots[0].ship.hp = 50.0;
    for _ in 0..60 {
        step(&mut w);
    }
    // One second of steps; the bay is far away, so this is the seal alone.
    assert!(
        (w.pilots[0].ship.hp - 51.5).abs() < 1e-6,
        "hull {}",
        w.pilots[0].ship.hp
    );
}

/// Training (guided-first-shift.js): rivals and the field event wait while
/// it runs; each step advances on its action after a short beat; skipping
/// ends it and lets the shift go on as normal.
#[test]
fn training_holds_rivals_and_advances_on_each_action() {
    let mut c = Career::new(Mode::Standard);
    c.shift = 1;
    let mut w = World::for_career(Rng::seeded(8), VIEW, &c);
    w.start_tutorial();
    w.event_done = false;
    w.event_t = 0.0;
    let timer = w.jumper_timer;
    for _ in 0..30 {
        step(&mut w);
    }
    assert_eq!(w.jumper_timer, timer, "no rivals while training");
    assert!(!w.event_done, "the field event waits");
    assert_eq!(w.tut.as_ref().map(|t| t.step), Some(0));

    // Step 1: fly clear of the bay, held for 0.8 s.
    w.pilots[0].ship.b.x = BAY_X + 600.0;
    w.pilots[0].ship.b.y = BAY_Y;
    for _ in 0..60 {
        w.pilots[0].ship.b.x = BAY_X + 600.0;
        w.pilots[0].ship.b.vx = 0.0;
        step(&mut w);
    }
    assert_eq!(w.tut.as_ref().map(|t| t.step), Some(1));

    w.end_tutorial(true);
    assert!(w.tut.is_none());
    step(&mut w);
    assert!(w.jumper_timer < timer, "rivals come once training ends");
}

/// Local co-op: the partner latches onto the cable's tail and pulls.
#[test]
fn local_coop_partner_latches_onto_the_cable_and_pulls() {
    use cj_sim::coop::P2Input;
    use cj_sim::Action;
    let mut c = Career::new(Mode::Standard);
    c.shift = 2;
    let mut w = quiet(&c, Rng::seeded(6));
    w.spawn_partner();
    w.pilots[0].ship.angle = 0.0;
    let (sx, sy) = (w.pilots[0].ship.b.x, w.pilots[0].ship.b.y);
    let it = w.new_item(ItemType::Engine, sx + 60.0, sy, 0.0, 0.0);
    let id = it.id;
    w.items.push(it);
    w.action(Action::Hook);
    step(&mut w);
    let i = w.item_index(id).expect("the engine");
    let ib = w.items[i].b;
    let p = w.p2.as_mut().expect("partner");
    p.b.x = ib.x + 60.0;
    p.b.y = ib.y;
    p.b.vx = 0.0;
    p.b.vy = 0.0;
    w.p2_input = P2Input {
        latch: true,
        ..P2Input::default()
    };
    step(&mut w);
    w.p2_input = P2Input::default();
    step(&mut w);
    assert!(w.p2.as_ref().unwrap().linked, "latched");

    let x0 = w.items[w.item_index(id).unwrap()].b.x;
    w.p2.as_mut().unwrap().angle = 0.0;
    w.p2_input = P2Input {
        thrust: 1.0,
        ..P2Input::default()
    };
    for _ in 0..90 {
        step(&mut w);
        w.pilots[0].ship.b.vx = 0.0;
        w.pilots[0].ship.b.vy = 0.0;
    }
    let x1 = w.items[w.item_index(id).unwrap()].b.x;
    assert!(x1 - x0 > 10.0, "pulled {x0} -> {x1}");
    assert!(w.p2.as_ref().unwrap().linked, "still latched");
}

/// A cargo smash freezes the action briefly and marks a highlight.
#[test]
fn a_cargo_smash_raises_hit_stop_and_a_highlight() {
    use cj_sim::events::Event;
    let mut c = Career::new(Mode::Standard);
    c.shift = 3;
    let mut w = quiet(&c, Rng::seeded(2));
    let (sx, sy) = (w.pilots[0].ship.b.x, w.pilots[0].ship.b.y);
    let j = w.new_jumper(sx + 200.0, sy, Some(cj_sim::consts::Variant::Cutter));
    w.jumpers.push(j);
    w.events.clear();
    let n = w.jumpers.len() - 1;
    w.hurt_jumper(n, 99.0, Some(7.0));
    assert!(w.heard(&Event::HitStop(HITSTOP_SMASH)));
    assert!(w.events.iter().map(|n| &n.event).any(
        |e| matches!(e, Event::Highlight { text, .. } if text == "Smashed a rival with cargo")
    ));
    assert!(w.heard(&Event::Unlock(cj_sim::events::Achievement::Smash)));
}

/// updateCamera in co-op: centred between the tugs, zoomed out to fit them.
#[test]
fn the_camera_frames_both_tugs() {
    let c = Career::new(Mode::Standard);
    let mut w = quiet(&c, Rng::Constant(0.5));
    w.rocks.clear();
    w.wrecks.clear();
    w.spawn_partner();
    let p = w.p2.as_mut().expect("partner");
    p.b.x = w.pilots[0].ship.b.x - 1500.0;
    p.b.y = w.pilots[0].ship.b.y;
    // Long enough for the camera's easing to settle.
    for _ in 0..900 {
        step(&mut w);
    }
    let p = w.p2.as_ref().expect("partner");
    let mid = (w.pilots[0].ship.b.x + p.b.x) / 2.0;
    assert!((w.cam.x - mid).abs() < 1.0, "{} vs {mid}", w.cam.x);
    let gap = (w.pilots[0].ship.b.x - p.b.x).hypot(w.pilots[0].ship.b.y - p.b.y);
    let base = (VIEW.0.min(VIEW.1) / 900.0).clamp(0.5, 1.1);
    let want = base * 750.0 / gap;
    assert!((w.cam.zoom - want).abs() < 0.01, "{} vs {want}", w.cam.zoom);
}

/// A quiet field with one rival, nearly dead, next to (x, y).
fn with_frail_rival(x: f64, y: f64) -> World {
    let c = Career::new(Mode::Standard);
    let mut w = quiet(&c, Rng::Constant(0.5));
    w.rocks.clear();
    w.wrecks.clear();
    let mut j = w.new_jumper(x, y, Some(Variant::Skiff));
    j.hp = 0.1;
    w.jumpers.push(j);
    w
}

fn no_credit_taken(w: &World) {
    assert_eq!(w.pilots[0].stats.jumpers, 0, "no kill on the shift report");
    assert_eq!(w.pilots[0].kills.skiff, 0, "no kill on the tally");
    assert_eq!(w.grudge, 0.0, "no grudge");
    assert_eq!(w.pilots[0].bounties, 0.0, "no bounty");
}

#[test]
fn a_rival_crashing_into_a_rock_is_not_your_kill() {
    let (x, y) = (BAY_X - 900.0, BAY_Y + 900.0);
    let mut w = with_frail_rival(x, y);
    let rock = make_rock_drifting(&mut w.rng, 3, x + 30.0, y);
    w.rocks.push(rock);
    w.jumpers[0].b.vx = 1500.0;
    for _ in 0..3 {
        step(&mut w);
    }
    assert!(w.jumpers.is_empty(), "the crash killed it");
    no_credit_taken(&w);
}

#[test]
fn only_a_reactor_you_set_off_earns_the_kill() {
    let (x, y) = (BAY_X - 900.0, BAY_Y + 900.0);
    for blamed in [false, true] {
        let mut w = with_frail_rival(x, y);
        let it = w.new_item(ItemType::Reactor, x + 80.0, y, 0.0, 0.0);
        w.items.push(it);
        let i = w.items.len() - 1;
        if blamed {
            w.prime_reactor(i);
        } else {
            w.prime_reactor_by(i, cj_sim::world::Blame::Nobody);
        }
        for _ in 0..10 {
            step(&mut w);
        }
        assert!(w.jumpers.is_empty(), "the blast killed it");
        if blamed {
            assert_eq!(w.pilots[0].stats.jumpers, 1);
        } else {
            no_credit_taken(&w);
        }
    }
}

#[test]
fn a_blast_splits_a_boulder_without_erasing_the_fragments() {
    let c = Career::new(Mode::Standard);
    let mut w = quiet(&c, Rng::Constant(0.5));
    w.rocks.clear();
    let (x, y) = (BAY_X - 900.0, BAY_Y + 900.0);
    let mut r = make_rock_drifting(&mut w.rng, 3, x, y);
    r.hp = 1.0;
    w.rocks.push(r);
    w.explode(x, y, 230.0, 0.0);
    let alive = w.rocks.iter().filter(|r| !r.dead).count();
    assert!(alive >= 2, "fragments survive: {alive}");
}

#[test]
fn a_crab_on_the_hull_lets_go_of_its_cargo() {
    use cj_sim::Action;
    let c = Career::new(Mode::Standard);
    let mut w = quiet(&c, Rng::Constant(0.5));
    w.pilots[0].ship.angle = 0.0;
    let (sx, sy) = (w.pilots[0].ship.b.x, w.pilots[0].ship.b.y);
    let it = w.new_item(ItemType::Engine, sx + 60.0, sy, 0.0, 0.0);
    let id = it.id;
    w.items.push(it);
    w.action(Action::Hook);
    step(&mut w);
    let mut crab = make_crab(&mut w.rng, CrabState::Crawl, Some(id), sx + 60.0, sy);
    crab.t = 0.9999;
    w.crabs.push(crab);
    step(&mut w);
    assert_eq!(w.crabs[0].state, CrabState::Latched);
    assert_eq!(w.crabs[0].host, None);
}

#[test]
fn the_refinery_drifts_around_the_carrier_not_at_it() {
    for seed in 1..40 {
        let mut w = World::empty(Rng::seeded(seed), VIEW);
        w.spawn_refinery();
        let r = w.refinery.as_ref().expect("spawned");
        let along = (r.b.x - CARRIER_X) * r.b.vx + (r.b.y - CARRIER_Y) * r.b.vy;
        assert!(along.abs() < 1e-6, "seed {seed}: {along}");
    }
}

#[test]
fn cargo_taken_in_an_arrest_counts_as_lost() {
    use cj_sim::Action;
    let c = Career::new(Mode::Standard);
    let mut w = quiet(&c, Rng::Constant(0.5));
    w.pilots[0].ship.angle = 0.0;
    let (sx, sy) = (w.pilots[0].ship.b.x, w.pilots[0].ship.b.y);
    let it = w.new_item(ItemType::Engine, sx + 60.0, sy, 0.0, 0.0);
    w.items.push(it);
    w.action(Action::Hook);
    step(&mut w);
    let cargo = w.line_value(0);
    assert!(cargo > 0.0);
    w.pilots[0].wanted = 1;
    w.spawn_sheriff(0, false);
    w.sheriffs[0].shakedown = false;
    w.arrest(0);
    assert_eq!(w.pilots[0].stats.lost, cargo);
}

#[test]
fn wrecked_jaws_are_tallied_even_in_a_no_blame_blast() {
    use cj_sim::opposition::RefineryState;
    let c = Career::new(Mode::Standard);
    let mut w = quiet(&c, Rng::Constant(0.5));
    w.spawn_refinery();
    w.refinery.as_mut().expect("spawned").state = RefineryState::Hungry;
    w.blame = cj_sim::world::Blame::Nobody;
    w.hurt_jaws(999.0);
    assert_eq!(w.pilots[0].kills.jaws, 1);
}

#[test]
fn bolts_hit_the_hauler_where_it_is_drawn() {
    let c = Career::new(Mode::Standard);
    let mut w = quiet(&c, Rng::Constant(0.5));
    w.start_event(Event::Convoy);
    let h = w.hauler.as_mut().expect("spawned");
    (h.b.x, h.b.y, h.angle) = (1000.0, 1000.0, 0.0);
    let hit = |h: &Hauler, x: f64, y: f64| h.hit_by_bolt(1000.0 + x, 1000.0 + y);
    assert!(
        hit(h, -36.0, 21.0),
        "container corner, outside the old circle"
    );
    assert!(hit(h, -45.0, 15.0), "tail corner, outside the old circle");
    assert!(hit(h, 39.0, 0.0), "nose tip");
    assert!(!hit(h, 36.0, 20.0), "beside the tapered nose");
    assert!(
        !hit(h, -45.0, 21.0),
        "the spine is narrower than the containers"
    );
    assert!(!hit(h, 0.0, 30.0), "clear of the hull");
    // Facing +y, the hull's length runs along y.
    h.angle = std::f64::consts::FRAC_PI_2;
    assert!(hit(h, 21.0, -36.0));
    assert!(!hit(h, -36.0, 21.0));
}

fn open_field() -> (World, f64, f64) {
    let c = Career::new(Mode::Standard);
    let mut w = quiet(&c, Rng::Constant(0.5));
    w.rocks.clear();
    w.wrecks.clear();
    w.items.clear();
    w.pilots[0].ship.angle = 0.0;
    w.pilots[0].ship.invuln = 0.0;
    let (x, y) = (w.pilots[0].ship.b.x, w.pilots[0].ship.b.y);
    (w, x, y)
}

#[test]
fn your_shots_pass_through_towed_cargo_but_not_loose_cargo() {
    use cj_sim::Action;
    let shot = |x: f64, y: f64| Bullet {
        x,
        y,
        vx: 100.0,
        life: 1.0,
        dmg: 1.0,
        ..Bullet::default()
    };
    let (mut w, sx, sy) = open_field();
    let it = w.new_item(ItemType::Engine, sx + 60.0, sy, 0.0, 0.0);
    w.items.push(it);
    w.action(Action::Hook);
    step(&mut w);
    let b = w.items[0].b;
    w.bullets.push(shot(b.x, b.y));
    step(&mut w);
    assert_eq!(w.bullets.len(), 1, "passes through the towed engine");

    let (mut w, sx, sy) = open_field();
    let it = w.new_item(ItemType::Engine, sx + 300.0, sy, 0.0, 0.0);
    w.items.push(it);
    let b = w.items[0].b;
    w.bullets.push(shot(b.x, b.y));
    step(&mut w);
    assert!(w.bullets.is_empty(), "stopped by loose cargo");
}

#[test]
fn the_wrecked_tug_throws_a_shockwave_nobody_blames_you_for() {
    let (mut w, sx, sy) = open_field();
    let mut j = w.new_jumper(sx + 100.0, sy, Some(Variant::Skiff));
    j.hp = 0.1;
    w.jumpers.push(j);
    let it = w.new_item(ItemType::Scrap, sx + 150.0, sy, 0.0, 0.0);
    w.items.push(it);
    w.kill_ship(0);
    assert!(w.jumpers[0].dead, "caught in the blast");
    assert!(w.items[0].b.vx > 0.0, "debris thrown outward");
    assert_eq!(w.pilots[0].stats.jumpers, 0);
    assert_eq!(w.grudge, 0.0);
}

#[test]
fn a_wanted_star_fades_only_while_nobody_is_chasing_you() {
    use cj_sim::opposition::SheriffState;
    let (mut w, _, _) = open_field();
    w.pilots[0].wanted = 2;
    for _ in 0..(WANTED_COOLDOWN / STEP).ceil() as usize + 2 {
        step(&mut w);
        w.sheriffs.clear();
    }
    assert_eq!(w.pilots[0].wanted, 1);
    w.spawn_sheriff(0, false);
    w.sheriffs[0].state = SheriffState::Pursuit;
    w.pilots[0].wanted_cool_t = 1.0;
    step(&mut w);
    assert_eq!(
        w.pilots[0].wanted_cool_t, WANTED_COOLDOWN,
        "a chase holds the clock"
    );
}

#[test]
fn radiation_kills_shields_burns_the_hull_and_fades() {
    let (mut w, sx, sy) = open_field();
    w.clouds.push(RadCloud {
        x: sx,
        y: sy,
        t: RAD_CLOUD_TIME,
    });
    let hp = w.pilots[0].ship.hp;
    step(&mut w);
    assert_eq!(w.pilots[0].ship.shield, 0.0);
    assert!(
        w.pilots[0].ship.hp < hp && w.pilots[0].ship.hp > hp - 1.0,
        "a slow burn: {}",
        w.pilots[0].ship.hp
    );
    assert!(w.pilots[0].in_cloud);
    w.clouds[0].t = STEP / 2.0;
    step(&mut w);
    assert!(w.clouds.is_empty());
}

#[test]
fn a_reactor_leaves_a_cloud_except_over_the_bay() {
    for (dx, want) in [(-900.0, 1), (0.0, 0)] {
        let (mut w, _, _) = open_field();
        let it = w.new_item(ItemType::Reactor, BAY_X + dx, BAY_Y + dx.abs(), 0.0, 0.0);
        w.items.push(it);
        w.prime_reactor(0);
        for _ in 0..5 {
            step(&mut w);
        }
        assert_eq!(w.clouds.len(), want);
    }
}

#[test]
fn a_level_5_cannon_fires_a_laser_that_lands_instantly() {
    let mut c = Career::new(Mode::Standard);
    c.levels.set(System::Cannon, 5);
    let mut w = quiet(&c, Rng::Constant(0.5));
    w.rocks.clear();
    w.wrecks.clear();
    w.pilots[0].ship.angle = 0.0;
    let (sx, sy) = (w.pilots[0].ship.b.x, w.pilots[0].ship.b.y);
    let mut r = make_rock_drifting(&mut w.rng, 3, sx + LASER_RANGE * 0.7, sy);
    r.hp = 500.0;
    w.rocks.push(r);
    let fire = Input {
        fire: true,
        ..Input::default()
    };
    w.step(&fire, STEP);
    assert!(w.rocks[0].hp < 500.0, "hit in the step it fired");
    assert!(w.bullets.is_empty());
    assert!(w
        .events
        .iter()
        .map(|n| &n.event)
        .any(|e| matches!(e, cj_sim::events::Event::Laser { .. })));
}

#[test]
fn light_rivals_sidestep_shots_and_brutes_do_not() {
    let sideways = |v: Variant| {
        let (mut w, sx, sy) = open_field();
        let (x, y) = (sx - 1200.0, sy + 900.0);
        let mut j = w.new_jumper(x, y, Some(v));
        j.hp = 50.0;
        j.hunter = true;
        w.jumpers.push(j);
        w.bullets.push(Bullet {
            x: x - 100.0,
            y,
            vx: 820.0,
            life: 1.0,
            dmg: 1.0,
            ..Bullet::default()
        });
        step(&mut w);
        w.jumpers[0].b.vy.abs()
    };
    assert!(sideways(Variant::Skiff) > sideways(Variant::Brute) + 100.0);
}

#[test]
fn some_missiles_lose_their_lock() {
    let launch = |roll: f64| {
        let mut c = Career::new(Mode::Standard);
        c.wallet = 1e6;
        c.buy_part(Part::Missiles).expect("bought");
        let mut w = quiet(&c, Rng::Constant(roll));
        w.rocks.clear();
        w.wrecks.clear();
        let (sx, sy) = (w.pilots[0].ship.b.x, w.pilots[0].ship.b.y);
        let j = w.new_jumper(sx + 300.0, sy, Some(Variant::Cutter));
        w.jumpers.push(j);
        for _ in 0..3 {
            step(&mut w);
        }
        w.missiles[0].tracking
    };
    assert_eq!(launch(0.5), 1.0);
    assert_eq!(launch(0.1), MISSILE_WILD_TRACKING);
}

#[test]
fn charges_return_only_when_none_are_left_and_the_vault_is_shut() {
    let (mut w, _, _) = open_field();
    w.spawn_refinery();
    let charges = |w: &World| {
        w.items
            .iter()
            .filter(|i| i.kind == ItemType::Charge)
            .count()
    };
    assert_eq!(charges(&w), 4);
    w.items.retain(|i| i.kind != ItemType::Charge);
    for _ in 0..(cj_sim::opposition::CHARGE_RESPAWN_TIME / STEP).ceil() as usize + 2 {
        step(&mut w);
    }
    assert_eq!(charges(&w), 4, "three empty sockets and a spare");

    let (mut w, _, _) = open_field();
    w.spawn_refinery();
    w.items.retain(|i| i.kind != ItemType::Charge);
    w.vault_opened = true;
    for _ in 0..(cj_sim::opposition::CHARGE_RESPAWN_TIME / STEP).ceil() as usize + 2 {
        step(&mut w);
    }
    assert_eq!(charges(&w), 0, "nothing left to open");
}
