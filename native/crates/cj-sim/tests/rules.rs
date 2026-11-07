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
    w.events.iter().any(|e| *e == Event::Sfx(s))
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
