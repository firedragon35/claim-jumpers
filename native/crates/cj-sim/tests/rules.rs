//! Gameplay rules that the golden traces don't exercise directly.

use cj_sim::consts::*;
use cj_sim::events::{Event, Sfx};
use cj_sim::items::ItemType;
use cj_sim::rng::Rng;
use cj_sim::{Action, Input, World};

const VIEW: (f64, f64) = (1280.0, 720.0);

/// An empty field with the tug parked at (1000, 1000), facing +x.
fn field() -> World {
    let mut w = World::empty(Rng::Constant(0.5), VIEW);
    w.ship.b.x = 1000.0;
    w.ship.b.y = 1000.0;
    w.ship.angle = 0.0;
    w
}

fn add(w: &mut World, kind: ItemType, x: f64, y: f64) -> u32 {
    let it = w.new_item(kind, x, y, 0.0, 0.0);
    let id = it.id;
    w.items.push(it);
    id
}

fn has_sfx(w: &World, s: Sfx) -> bool {
    w.events.contains(&Event::Sfx(s))
}

#[test]
fn hook_takes_nearest_item_in_range() {
    let mut w = field();
    let far = add(&mut w, ItemType::Scrap, 1300.0, 1000.0); // 288 past its edge
    let near = add(&mut w, ItemType::Scrap, 1100.0, 1000.0); // 88 past its edge
    w.action(Action::Hook);
    assert_eq!(w.chain, vec![near]);
    w.action(Action::Hook);
    assert_eq!(w.chain, vec![near], "the far piece is out of reach");
    assert!(has_sfx(&w, Sfx::Deny));
    let _ = far;
}

#[test]
fn winch_refuses_more_than_its_capacity() {
    let mut w = field();
    // Two engine blocks are 14 t; a third would exceed the 16 t winch.
    for dy in [-60.0, 0.0, 60.0] {
        add(&mut w, ItemType::Engine, 1080.0, 1000.0 + dy);
    }
    for _ in 0..3 {
        w.action(Action::Hook);
    }
    assert_eq!(w.chain.len(), 2);
    assert!(w.tow_mass() <= WINCH_CAPACITY);
}

#[test]
fn winch_reels_link_down_to_its_minimum() {
    let mut w = field();
    let id = add(&mut w, ItemType::Scrap, 1140.0, 1000.0);
    w.action(Action::Hook);
    for _ in 0..120 {
        w.step(&Input::default(), STEP);
    }
    let it = &w.items[w.item_index(id).unwrap()];
    assert_eq!(it.link_len, it.b.r + SHIP_RADIUS + LINK_SLACK);
}

#[test]
fn drop_throws_and_cut_frees_everything() {
    let mut w = field();
    add(&mut w, ItemType::Scrap, 1080.0, 1000.0);
    add(&mut w, ItemType::Ore, 1000.0, 1100.0);
    w.action(Action::Hook);
    w.action(Action::Hook);
    w.action(Action::Drop);
    assert_eq!(w.chain.len(), 1);
    assert!(w.items.iter().any(|it| !it.towed && it.thrown_t == 1.6));
    w.action(Action::Cut);
    assert!(w.chain.is_empty());
    assert!(w.items.iter().all(|it| !it.towed));
}

#[test]
fn bay_banks_cargo_and_merges_quick_deliveries() {
    let mut w = field();
    add(&mut w, ItemType::Scrap, BAY_X, BAY_Y); // 80 cr at the 0.5 roll
    add(&mut w, ItemType::Ore, BAY_X + 10.0, BAY_Y); // 170 cr
    w.step(&Input::default(), STEP);
    assert_eq!(w.banked, 250.0);
    assert!(w.items.is_empty());
    assert_eq!(w.stats.items, 2);
    assert_eq!(
        w.stats.best_run, 250.0,
        "both landed inside the delivery window"
    );
}

#[test]
fn bay_repairs_a_parked_tug() {
    let mut w = field();
    w.ship.b.x = BAY_X;
    w.ship.b.y = BAY_Y;
    w.ship.hp = 50.0;
    for _ in 0..60 {
        w.step(&Input::default(), STEP);
    }
    assert!((w.ship.hp - (50.0 + BAY_REPAIR_RATE)).abs() < 1e-9);
}

#[test]
fn shield_absorbs_before_hull() {
    let mut w = field();
    w.ship.invuln = 0.0;
    w.damage_ship(30.0);
    assert_eq!(w.ship.shield, 10.0);
    assert_eq!(w.ship.hp, SHIP_MAX_HP);
    w.damage_ship(30.0);
    assert_eq!(w.ship.shield, 0.0);
    assert_eq!(w.ship.hp, SHIP_MAX_HP - 20.0);
}

#[test]
fn invulnerable_tug_takes_no_damage() {
    let mut w = field();
    w.damage_ship(500.0);
    assert!(w.ship.alive);
    assert_eq!(w.ship.hp, SHIP_MAX_HP);
}

#[test]
fn losing_the_tug_charges_the_deductible_and_respawns_at_the_bay() {
    let mut w = field();
    w.banked = 800.0;
    add(&mut w, ItemType::Scrap, 1080.0, 1000.0);
    w.action(Action::Hook);
    w.ship.invuln = 0.0;
    w.ship.shield = 0.0;
    w.damage_ship(1000.0);
    assert!(!w.ship.alive);
    assert_eq!(w.banked, 300.0);
    assert!(w.chain.is_empty(), "cargo floats free");
    for _ in 0..(RESPAWN_TIME / STEP) as usize + 2 {
        w.step(&Input::default(), STEP);
    }
    assert!(w.ship.alive);
    assert_eq!((w.ship.b.x, w.ship.b.y), (BAY_X, BAY_Y));
    assert_eq!(w.ship.hp, SHIP_MAX_HP);
}

#[test]
fn shift_ends_when_the_clock_runs_out() {
    let mut w = field();
    w.time_left = STEP * 1.5;
    w.step(&Input::default(), STEP);
    assert!(!w.shift_over);
    w.step(&Input::default(), STEP);
    assert!(w.shift_over);
}

#[test]
fn seeded_field_is_reproducible_and_full() {
    let a = World::new_shift(Rng::seeded(42), VIEW);
    let b = World::new_shift(Rng::seeded(42), VIEW);
    assert_eq!(a.rocks.len(), START_ROCKS);
    assert_eq!(a.wrecks.len(), START_WRECKS);
    assert_eq!(a.items.len(), START_ITEMS);
    let pos = |w: &World| w.rocks.iter().map(|r| (r.b.x, r.b.y)).collect::<Vec<_>>();
    assert_eq!(pos(&a), pos(&b));
    // Nothing spawns inside the carrier's safe zone.
    assert!(a
        .rocks
        .iter()
        .all(|r| cj_sim::js::dist(r.b.x, r.b.y, CARRIER_X, CARRIER_Y) >= 650.0));
}

#[test]
fn empty_winch_weighs_positive_zero() {
    // The HUD prints this; a -0.0 would read "-0 / 16 t".
    let w = field();
    assert!(w.tow_mass() == 0.0 && w.tow_mass().is_sign_positive());
    assert!(w.line_value().is_sign_positive());
}

// ---------------------------------------------------------------------
// Combat (milestone 2).
// ---------------------------------------------------------------------

use cj_sim::consts::Variant;
use cj_sim::entities::{make_crab, CrabState, JumperState};

fn firing() -> Input {
    Input {
        fire: true,
        ..Input::default()
    }
}

#[test]
fn stock_cannon_never_overheats_but_a_hot_loadout_does() {
    let mut w = field();
    for _ in 0..600 {
        w.step(&firing(), STEP);
    }
    assert!(
        !w.ship.overheated,
        "one barrel sheds heat as fast as it builds"
    );
    // A twin cutter with a level-4 spread (three barrels, four extra shots).
    w.mods.barrels = vec![0.0, -7.0, 7.0];
    w.mods.cannon_spread = 4;
    let mut locked = false;
    for _ in 0..600 {
        w.step(&firing(), STEP);
        locked |= w.ship.overheated;
    }
    assert!(locked);
    let shots = w.shots_fired;
    // While locked, the trigger does nothing until heat falls below the resume mark.
    w.ship.overheated = true;
    w.ship.heat = 100.0;
    w.step(&firing(), STEP);
    assert_eq!(w.shots_fired, shots);
}

#[test]
fn hard_impact_primes_a_reactor_which_then_blows() {
    let mut w = field();
    let id = add(&mut w, ItemType::Reactor, 2000.0, 1000.0);
    w.items[0].b.vx = 500.0;
    add(&mut w, ItemType::Scrap, 2050.0, 1000.0);
    // It closes the gap in a few steps; the hit primes it.
    let primed = (0..20).any(|_| {
        w.step(&Input::default(), STEP);
        w.item_index(id).is_some_and(|i| w.items[i].fuse > 0.0)
    });
    assert!(primed);
    for _ in 0..5 {
        w.step(&Input::default(), STEP);
    }
    assert!(w.item_index(id).is_none(), "the fuse ran out");
    assert!(w
        .events
        .iter()
        .any(|e| matches!(e, Event::Explosion { .. })));
}

#[test]
fn a_rival_escaping_with_loot_counts_it_lost() {
    let mut w = field();
    add(&mut w, ItemType::Ore, 100.0, 1000.0);
    let j = w.new_jumper(60.0, 1000.0, Some(Variant::Cutter));
    w.jumpers.push(j);
    let mut hauled = false;
    for _ in 0..1200 {
        w.step(&Input::default(), STEP);
        hauled |= w.jumpers.iter().any(|j| j.state == JumperState::Haul);
        if w.jumpers.is_empty() {
            break;
        }
    }
    assert!(hauled);
    assert!(
        w.jumpers.is_empty() && w.items.is_empty(),
        "it left with the ore"
    );
    assert_eq!(w.stats.lost, 170.0);
}

#[test]
fn rivals_arrive_on_the_timer_up_to_the_cap() {
    let mut w = field();
    w.jumper_timer = 0.0;
    w.step(&Input::default(), STEP);
    assert_eq!(w.jumpers.len(), 1);
    assert!(w.jumper_timer > 0.0, "the next arrival is scheduled");
    // Shift 1 allows two at once.
    for _ in 0..5 {
        w.jumper_timer = 0.0;
        w.step(&Input::default(), STEP);
    }
    assert_eq!(w.jumpers.len(), 2);
}

#[test]
fn scrapping_rivals_raises_the_grudge_stage() {
    let mut w = field();
    w.grudge = 1.0;
    for _ in 0..1 {
        let j = w.new_jumper(3000.0, 3000.0, Some(Variant::Skiff));
        w.jumpers.push(j);
    }
    w.hurt_jumper(0, 99.0, None);
    assert_eq!(w.rival_stage(), 1, "Annoyed at grudge 2");
    assert_eq!(w.banked, 100.0, "skiff bounty");
    assert_eq!(w.kills.skiff, 1);
    let popup = |w: &World| {
        w.events
            .iter()
            .any(|e| matches!(e, Event::Popup { text, .. } if text == "RIVALS: ANNOYED"))
    };
    assert!(popup(&w));
}

#[test]
fn the_bay_pries_latched_crabs_off_and_banking_kills_riders() {
    let mut w = field();
    w.ship.b.x = BAY_X;
    w.ship.b.y = BAY_Y + 150.0; // inside repair range, outside the ring
    let c = make_crab(&mut w.rng, CrabState::Latched, None, BAY_X, BAY_Y);
    w.crabs.push(c);
    let id = add(&mut w, ItemType::Scrap, BAY_X, BAY_Y);
    let c = make_crab(&mut w.rng, CrabState::Ride, Some(id), BAY_X, BAY_Y);
    w.crabs.push(c);
    w.step(&Input::default(), STEP);
    assert!(w.crabs.is_empty());
}

#[test]
fn losing_the_tug_throws_its_crabs_off() {
    let mut w = field();
    let c = make_crab(&mut w.rng, CrabState::Latched, None, 1000.0, 1000.0);
    w.crabs.push(c);
    w.ship.invuln = 0.0;
    w.ship.shield = 0.0;
    w.damage_ship(1000.0);
    assert_eq!(w.crabs[0].state, CrabState::Free);
}

#[test]
fn under_attack_alarm_is_rate_limited() {
    let mut w = field();
    w.ship.invuln = 0.0;
    let alarms = |w: &World| {
        w.events
            .iter()
            .filter(|e| **e == Event::Sfx(Sfx::AttackAlarm))
            .count()
    };
    let mut total = 0;
    // A bolt every half second for four seconds: one klaxon, warning held.
    for k in 0..240 {
        if k % 30 == 0 {
            w.ebullets.push(cj_sim::entities::Bullet {
                x: 1000.0,
                y: 1000.0,
                vx: 0.0,
                vy: 0.0,
                life: 1.0,
                dmg: 5.0,
                slug: false,
                ..cj_sim::entities::Bullet::default()
            });
        }
        w.events.clear();
        w.step(&Input::default(), STEP);
        total += alarms(&w);
        assert!(k < 1 || w.under_attack_t > 0.0);
    }
    assert_eq!(total, 1);
}

#[test]
fn shooting_a_wreck_ejects_its_contents_then_breaks_it() {
    let mut w = World::empty(Rng::seeded(3), VIEW);
    let wr = cj_sim::entities::make_wreck(&mut w.rng, 2000.0, 2000.0, 1);
    let n = wr.contents.len();
    w.wrecks.push(wr);
    let mut hits = 0;
    while !w.wrecks.is_empty() {
        w.damage_wreck(0, 1.0);
        hits += 1;
        w.wrecks.retain(|x| !x.dead);
        assert!(hits < 200);
    }
    assert_eq!(w.items.len(), n, "every piece came out");
    assert_eq!(w.rocks.len(), 3, "the hull left pebbles");
}
