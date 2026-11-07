//! The browser's own tests for the career's field systems
//! (tests/specs/phases-6-9.spec.js in the repository root), ported to the
//! Rust simulation: standing and truces, contract chains, belt conditions,
//! escorts, sealed holds and rogue cores. Each keeps the browser test's
//! setup and assertions. The presentation, daily and co-op tests in that
//! file arrive with their systems.

use cj_sim::career::{Career, Mode};
use cj_sim::consts::*;
use cj_sim::field::*;
use cj_sim::items::ItemType;
use cj_sim::parts::Part;
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
    w.ship.b.x = BAY_X + 1400.0;
    w.ship.b.y = BAY_Y;
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
    w.banked = 9000.0;
    c.settle(&w);
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
        w.ship.hp = 999.0;
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
    let k = w.contract.clone().unwrap();
    assert!(w.escort_done);
    assert!(w.contract_progress(&k) >= k.n);
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
    let mut d = w.make_sentry(0, 1);
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
    w.banked = 5000.0;
    c.settle(&w);
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
    w.banked = 4000.0;
    c.settle(&w);
    assert!(c.owns(Part::HullSeal), "the chain's last step awards it");
    c.mount(Part::HullSeal).expect("an awarded part mounts");

    let mut w = quiet(&c, Rng::seeded(3));
    assert!(w.mods.hull_seal);
    w.ship.hp = 50.0;
    for _ in 0..60 {
        step(&mut w);
    }
    // One second of steps; the bay is far away, so this is the seal alone.
    assert!((w.ship.hp - 51.5).abs() < 1e-6, "hull {}", w.ship.hp);
}
