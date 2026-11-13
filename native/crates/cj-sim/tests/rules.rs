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
    w.pilots[0].ship.b.x = 1000.0;
    w.pilots[0].ship.b.y = 1000.0;
    w.pilots[0].ship.angle = 0.0;
    w
}

fn add(w: &mut World, kind: ItemType, x: f64, y: f64) -> u32 {
    let it = w.new_item(kind, x, y, 0.0, 0.0);
    let id = it.id;
    w.items.push(it);
    id
}

fn has_sfx(w: &World, s: Sfx) -> bool {
    w.heard(&Event::Sfx(s))
}

#[test]
fn hook_takes_nearest_item_in_range() {
    let mut w = field();
    let far = add(&mut w, ItemType::Scrap, 1300.0, 1000.0); // 288 past its edge
    let near = add(&mut w, ItemType::Scrap, 1100.0, 1000.0); // 88 past its edge
    w.action(Action::Hook);
    assert_eq!(w.pilots[0].chain, vec![near]);
    w.action(Action::Hook);
    assert_eq!(
        w.pilots[0].chain,
        vec![near],
        "the far piece is out of reach"
    );
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
    assert_eq!(w.pilots[0].chain.len(), 2);
    assert!(w.tow_mass(0) <= WINCH_CAPACITY);
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
    assert_eq!(w.pilots[0].chain.len(), 1);
    assert!(w.items.iter().any(|it| !it.towed && it.thrown_t == 1.6));
    w.action(Action::Cut);
    assert!(w.pilots[0].chain.is_empty());
    assert!(w.items.iter().all(|it| !it.towed));
}

#[test]
fn bay_banks_cargo_and_merges_quick_deliveries() {
    let mut w = field();
    add(&mut w, ItemType::Scrap, BAY_X, BAY_Y); // 80 cr at the 0.5 roll
    add(&mut w, ItemType::Ore, BAY_X + 10.0, BAY_Y); // 170 cr
    w.step(&Input::default(), STEP);
    assert_eq!(w.pilots[0].banked, 250.0);
    assert!(w.items.is_empty());
    assert_eq!(w.pilots[0].stats.items, 2);
    assert_eq!(
        w.pilots[0].stats.best_run, 250.0,
        "both landed inside the delivery window"
    );
}

#[test]
fn bay_repairs_a_parked_tug() {
    let mut w = field();
    w.pilots[0].ship.b.x = BAY_X;
    w.pilots[0].ship.b.y = BAY_Y;
    w.pilots[0].ship.hp = 50.0;
    for _ in 0..60 {
        w.step(&Input::default(), STEP);
    }
    assert!((w.pilots[0].ship.hp - (50.0 + BAY_REPAIR_RATE)).abs() < 1e-9);
}

#[test]
fn shield_absorbs_before_hull() {
    let mut w = field();
    w.pilots[0].ship.invuln = 0.0;
    w.damage_ship(0, 30.0);
    assert_eq!(w.pilots[0].ship.shield, 10.0);
    assert_eq!(w.pilots[0].ship.hp, SHIP_MAX_HP);
    w.damage_ship(0, 30.0);
    assert_eq!(w.pilots[0].ship.shield, 0.0);
    assert_eq!(w.pilots[0].ship.hp, SHIP_MAX_HP - 20.0);
}

#[test]
fn invulnerable_tug_takes_no_damage() {
    let mut w = field();
    w.damage_ship(0, 500.0);
    assert!(w.pilots[0].ship.alive);
    assert_eq!(w.pilots[0].ship.hp, SHIP_MAX_HP);
}

#[test]
fn losing_the_tug_charges_the_deductible_and_respawns_at_the_bay() {
    let mut w = field();
    w.pilots[0].banked = 800.0;
    add(&mut w, ItemType::Scrap, 1080.0, 1000.0);
    w.action(Action::Hook);
    w.pilots[0].ship.invuln = 0.0;
    w.pilots[0].ship.shield = 0.0;
    w.damage_ship(0, 1000.0);
    assert!(!w.pilots[0].ship.alive);
    assert_eq!(w.pilots[0].banked, 300.0);
    assert!(w.pilots[0].chain.is_empty(), "cargo floats free");
    for _ in 0..(RESPAWN_TIME / STEP) as usize + 2 {
        w.step(&Input::default(), STEP);
    }
    assert!(w.pilots[0].ship.alive);
    assert_eq!((w.pilots[0].ship.b.x, w.pilots[0].ship.b.y), (BAY_X, BAY_Y));
    assert_eq!(w.pilots[0].ship.hp, SHIP_MAX_HP);
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
    assert!(w.tow_mass(0) == 0.0 && w.tow_mass(0).is_sign_positive());
    assert!(w.line_value(0).is_sign_positive());
}

// ---------------------------------------------------------------------
// Combat.
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
        !w.pilots[0].ship.overheated,
        "one barrel sheds heat as fast as it builds"
    );
    // A twin cutter with a level-4 spread (three barrels, four extra shots).
    w.pilots[0].mods.barrels = vec![0.0, -7.0, 7.0];
    w.pilots[0].mods.cannon_spread = 4;
    let mut locked = false;
    for _ in 0..600 {
        w.step(&firing(), STEP);
        locked |= w.pilots[0].ship.overheated;
    }
    assert!(locked);
    let shots = w.pilots[0].shots_fired;
    // While locked, the trigger does nothing until heat falls below the resume mark.
    w.pilots[0].ship.overheated = true;
    w.pilots[0].ship.heat = 100.0;
    w.step(&firing(), STEP);
    assert_eq!(w.pilots[0].shots_fired, shots);
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
        .map(|n| &n.event)
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
    assert_eq!(w.pilots[0].stats.lost, 170.0);
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
    assert_eq!(w.pilots[0].banked, 100.0, "skiff bounty");
    assert_eq!(w.pilots[0].kills.skiff, 1);
    let popup = |w: &World| {
        w.events
            .iter()
            .map(|n| &n.event)
            .any(|e| matches!(e, Event::Popup { text, .. } if text == "RIVALS: ANNOYED"))
    };
    assert!(popup(&w));
}

#[test]
fn the_bay_pries_latched_crabs_off_and_banking_kills_riders() {
    let mut w = field();
    w.pilots[0].ship.b.x = BAY_X;
    w.pilots[0].ship.b.y = BAY_Y + 150.0; // inside repair range, outside the ring
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
    w.pilots[0].ship.invuln = 0.0;
    w.pilots[0].ship.shield = 0.0;
    w.damage_ship(0, 1000.0);
    assert_eq!(w.crabs[0].state, CrabState::Free);
}

#[test]
fn under_attack_alarm_is_rate_limited() {
    let mut w = field();
    w.pilots[0].ship.invuln = 0.0;
    let alarms = |w: &World| {
        w.events
            .iter()
            .map(|n| &n.event)
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
        assert!(k < 1 || w.pilots[0].under_attack_t > 0.0);
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

/// The field above with a second pilot's tug parked at (2000, 1000), facing +x.
fn two_pilot_field() -> World {
    let mut w = field();
    let mut second = cj_sim::world::Pilot::new(w.pilots[0].mods.clone());
    second.ship.b.x = 2000.0;
    second.ship.b.y = 1000.0;
    second.ship.angle = 0.0;
    w.pilots.push(second);
    w
}

/// Each pilot's hook lands on that pilot's own chain, and dropping cargo
/// only touches the pilot who dropped it.
#[test]
fn each_pilot_hooks_onto_its_own_chain() {
    let mut w = two_pilot_field();
    let near_first = add(&mut w, ItemType::Scrap, 1100.0, 1000.0);
    let near_second = add(&mut w, ItemType::Scrap, 2100.0, 1000.0);
    w.action_for(1, Action::Hook);
    w.action_for(0, Action::Hook);
    assert_eq!(w.pilots[0].chain, vec![near_first]);
    assert_eq!(w.pilots[1].chain, vec![near_second]);
    assert!(w.tow_mass(1) > 0.0);
    w.action_for(1, Action::Drop);
    assert!(w.pilots[1].chain.is_empty());
    assert_eq!(
        w.pilots[0].chain,
        vec![near_first],
        "pilot 0 keeps its cargo"
    );
}

/// Each pilot's controls fly only that pilot's tug.
#[test]
fn each_input_flies_its_own_tug() {
    let mut w = two_pilot_field();
    let thrust = Input {
        thrust: 1.0,
        ..Input::default()
    };
    for _ in 0..30 {
        w.step_all(&[Input::default(), thrust], STEP);
    }
    assert_eq!(w.pilots[0].ship.b.x, 1000.0, "pilot 0 had no thrust");
    assert!(w.pilots[1].ship.b.x > 2000.0, "pilot 1 thrust along +x");
}

/// Removing a towed item from the field takes it off whichever chain held it.
#[test]
fn removing_cargo_finds_the_chain_that_holds_it() {
    let mut w = two_pilot_field();
    let id = add(&mut w, ItemType::Scrap, 2100.0, 1000.0);
    w.action_for(1, Action::Hook);
    w.remove_from_chain(id);
    assert!(w.pilots[1].chain.is_empty());
    assert!(!w.items[0].towed);
}

/// Stepping with the wrong number of inputs is a caller bug and panics.
#[test]
#[should_panic(expected = "one input per pilot")]
fn step_all_needs_one_input_per_pilot() {
    let mut w = two_pilot_field();
    w.step_all(&[Input::default()], STEP);
}

/// Two tugs that ram each other bounce apart without hull damage.
#[test]
fn tugs_ramming_each_other_take_no_damage() {
    let mut w = two_pilot_field();
    w.pilots[1].ship.b.x = 1040.0;
    w.pilots[0].ship.b.vx = 400.0;
    w.pilots[1].ship.b.vx = -400.0;
    // New tugs start with respawn grace, which would hide any damage.
    w.pilots[0].ship.invuln = 0.0;
    w.pilots[1].ship.invuln = 0.0;
    // The shield soaks a hit first, so check it along with the hull.
    let health = |w: &World| {
        let (a, b) = (&w.pilots[0].ship, &w.pilots[1].ship);
        (a.hp, a.shield, b.hp, b.shield)
    };
    let before = health(&w);
    for _ in 0..10 {
        w.step_all(&[Input::default(), Input::default()], STEP);
    }
    assert_eq!(health(&w), before);
    assert!(
        w.pilots[0].ship.b.vx < 400.0,
        "the collision pushed the tugs apart"
    );
}

/// Wrecking one pilot's tug charges that pilot's deductible and counts that
/// pilot's death; the other pilot's bank and record are untouched.
#[test]
fn a_wreck_charges_only_its_own_pilot() {
    let mut w = two_pilot_field();
    w.pilots[0].banked = 1000.0;
    w.pilots[1].banked = 1000.0;
    w.kill_ship(1);
    assert!(w.pilots[1].banked < 1000.0, "pilot 1 paid the deductible");
    assert_eq!(w.pilots[1].stats.deaths, 1);
    assert_eq!(w.pilots[0].banked, 1000.0);
    assert_eq!(w.pilots[0].stats.deaths, 0);
}

/// Each pilot's hull damage uses that pilot's own difficulty and shield.
#[test]
fn damage_uses_the_pilots_own_difficulty() {
    let mut w = two_pilot_field();
    for pl in &mut w.pilots {
        pl.ship.invuln = 0.0;
        pl.ship.shield = 0.0;
    }
    w.pilots[1].diff_damage = 2.0;
    w.damage_ship(0, 10.0);
    w.damage_ship(1, 10.0);
    let lost = |p: usize| w.pilots[p].mods.max_hp - w.pilots[p].ship.hp;
    assert_eq!(lost(0), 10.0);
    assert_eq!(lost(1), 20.0);
}

/// A rival a pilot's round destroys is that pilot's kill: the tally, the
/// bounty and the grudge-stage credit go to the shooter only.
#[test]
fn a_rounds_kill_is_credited_to_its_shooter() {
    let mut w = two_pilot_field();
    let j = w.new_jumper(3000.0, 1000.0, None);
    w.jumpers.push(j);
    w.bullets.push(cj_sim::entities::Bullet {
        x: 3000.0,
        y: 1000.0,
        vx: 1.0,
        life: 1.0,
        dmg: 999.0,
        blame: cj_sim::world::Blame::Pilot(1),
        ..Default::default()
    });
    w.step_all(&[Input::default(), Input::default()], STEP);
    assert!(w.jumpers.iter().all(|j| j.dead) || w.jumpers.is_empty());
    assert_eq!(w.pilots[1].stats.jumpers, 1, "the shooter scored it");
    assert_eq!(w.pilots[0].stats.jumpers, 0);
    assert!(w.pilots[1].bounties > 0.0);
    assert_eq!(w.pilots[0].bounties, 0.0);
}

/// With several pilots, damage nothing traced to a pilot credits nobody
/// (in single-player the same damage is the player's, as in the browser).
#[test]
fn untraced_damage_credits_nobody_with_several_pilots() {
    let mut w = two_pilot_field();
    let j = w.new_jumper(3000.0, 1000.0, None);
    w.jumpers.push(j);
    w.hurt_jumper(0, 999.0, None);
    assert_eq!(w.pilots[0].stats.jumpers + w.pilots[1].stats.jumpers, 0);

    let mut solo = field();
    let j = solo.new_jumper(3000.0, 1000.0, None);
    solo.jumpers.push(j);
    solo.hurt_jumper(0, 999.0, None);
    assert_eq!(solo.pilots[0].stats.jumpers, 1);
}

/// Wanted stars go to the pilot who committed the crime.
#[test]
fn wanted_stars_go_to_the_offending_pilot() {
    let mut w = two_pilot_field();
    w.add_wanted(1, 2, 2000.0, 1000.0);
    assert_eq!(w.pilots[1].wanted, 2);
    assert_eq!(w.pilots[0].wanted, 0);
}

/// A collector sent for one pilot nets that pilot's tug, not the other's.
#[test]
fn a_collector_nets_the_pilot_it_came_for() {
    let mut w = two_pilot_field();
    w.spawn_collector(1);
    assert_eq!(w.collectors[0].target, 1);
    w.pilots[1].ship.invuln = 0.0;
    w.net_ship(0);
    assert!(w.pilots[1].ship.net.is_some(), "pilot 1 is netted");
    assert!(w.pilots[0].ship.net.is_none(), "pilot 0 flies free");
}

/// Clearing one pilot's record sends home only the units after that pilot.
#[test]
fn clearing_a_record_only_calls_off_that_pilots_pursuit() {
    use cj_sim::opposition::SheriffState;
    let mut w = two_pilot_field();
    w.pilots[0].wanted = 1;
    w.pilots[1].wanted = 1;
    w.spawn_sheriff(0, true);
    w.spawn_sheriff(1, true);
    for sh in &mut w.sheriffs {
        sh.state = SheriffState::Pursuit;
    }
    w.clear_record(1);
    assert_eq!(w.pilots[1].wanted, 0);
    assert_eq!(w.pilots[0].wanted, 1);
    assert_eq!(
        w.sheriffs[0].state,
        SheriffState::Pursuit,
        "pilot 0 is still chased"
    );
    assert_eq!(w.sheriffs[1].state, SheriffState::Leave);
}

/// When a tug is lost, only that pilot's sentries go rogue.
#[test]
fn only_the_lost_pilots_sentries_go_rogue() {
    let mut w = two_pilot_field();
    for p in 0..2 {
        let s = w.make_sentry(p, 0, 1);
        w.sentries.push(s);
    }
    w.kill_ship(1);
    assert!(!w.sentries[0].rogue);
    assert!(w.sentries[1].rogue);
}

/// A patrol watches whichever tug is nearest.
#[test]
fn a_patrol_watches_the_nearest_tug() {
    let mut w = two_pilot_field();
    w.spawn_sheriff(0, false);
    w.sheriffs[0].b.x = 2100.0;
    w.sheriffs[0].b.y = 1000.0;
    w.step_all(&[Input::default(), Input::default()], STEP);
    assert_eq!(w.sheriffs[0].target, 1);
}

/// Law and rival bolts hit whichever tug is in the crossfire: a bystander
/// takes the damage, and a stun bolt slows them too.
#[test]
fn bolts_hit_bystanders_in_the_crossfire() {
    for stun in [true, false] {
        let mut w = two_pilot_field();
        for pl in &mut w.pilots {
            pl.ship.invuln = 0.0;
        }
        let shield = w.pilots[1].ship.shield;
        w.ebullets.push(cj_sim::entities::Bullet {
            x: 2000.0,
            y: 1000.0,
            vx: 1.0,
            life: 1.0,
            dmg: 10.0,
            stun,
            ..Default::default()
        });
        w.step_all(&[Input::default(), Input::default()], STEP);
        assert!(
            w.pilots[1].ship.shield < shield,
            "the bystander took the hit"
        );
        assert_eq!(w.pilots[1].ship.stun_t > 0.0, stun);
        assert!(w.ebullets.is_empty(), "the bolt was spent on them");
    }
}

/// A pilot's own reactor blast hurts their own tug but not another pilot's.
#[test]
fn a_pilots_blast_spares_other_players() {
    let mut w = two_pilot_field();
    for pl in &mut w.pilots {
        pl.ship.invuln = 0.0;
        pl.ship.shield = 0.0;
    }
    w.pilots[1].ship.b.x = 1100.0;
    let hp = w.pilots[1].ship.hp;
    w.blame = cj_sim::world::Blame::Pilot(0);
    w.explode(1050.0, 1000.0, 300.0, 30.0);
    assert!(w.pilots[0].ship.hp < hp, "the blast hurt its own pilot");
    assert_eq!(w.pilots[1].ship.hp, hp, "and spared the other");
}

/// Cargo banked at the bay pays the pilot who hooked it.
#[test]
fn banked_cargo_pays_the_pilot_who_hooked_it() {
    let mut w = two_pilot_field();
    add(&mut w, ItemType::Scrap, 2100.0, 1000.0);
    w.action_for(1, Action::Hook);
    w.bank_item(0);
    assert!(w.pilots[1].banked > 0.0);
    assert_eq!(w.pilots[0].banked, 0.0);
}

/// The bay repairs every tug parked at it.
#[test]
fn the_bay_repairs_each_parked_tug() {
    let mut w = two_pilot_field();
    w.pilots[1].ship.b.x = BAY_X;
    w.pilots[1].ship.b.y = BAY_Y;
    w.pilots[1].ship.hp = 50.0;
    w.step_all(&[Input::default(), Input::default()], STEP);
    assert!(w.pilots[1].ship.hp > 50.0);
}

/// A loose crab goes for the nearest tug and latches onto that pilot.
#[test]
fn a_loose_crab_latches_onto_the_nearest_tug() {
    use cj_sim::entities::{make_crab, CrabState};
    let mut w = two_pilot_field();
    let crab = make_crab(&mut w.rng, CrabState::Free, None, 2010.0, 1000.0);
    w.crabs.push(crab);
    w.step_all(&[Input::default(), Input::default()], STEP);
    assert_eq!(w.crabs[0].state, CrabState::Latched);
    assert_eq!(w.crabs[0].on, 1);
}

/// Radiation cooks any tug passing through it, whoever's reactor made the
/// cloud, even though the blast itself spared that pilot.
#[test]
fn radiation_affects_every_tug_inside() {
    let mut w = two_pilot_field();
    for pl in &mut w.pilots {
        pl.ship.invuln = 0.0;
    }
    add(&mut w, ItemType::Reactor, 2000.0, 1060.0);
    w.prime_reactor_by(0, cj_sim::world::Blame::Pilot(0));
    let hp = w.pilots[1].ship.hp;
    for _ in 0..30 {
        w.step_all(&[Input::default(), Input::default()], STEP);
    }
    assert!(!w.clouds.is_empty(), "the reactor left a cloud");
    assert!(w.pilots[1].in_cloud, "pilot 1 is inside it");
    assert!(w.pilots[1].ship.hp < hp, "and its hull is cooking");
    assert_eq!(w.pilots[1].ship.shield, 0.0);
}

/// A security drone with two tugs in range rotates its volleys between them.
#[test]
fn a_drone_rotates_its_volleys_between_tugs_in_range() {
    use cj_sim::entities::Body;
    use cj_sim::opposition::Drone;
    let mut w = two_pilot_field();
    w.drones.push(Drone {
        emp_t: 0.0,
        id: 999,
        b: Body {
            x: 1500.0,
            y: 1000.0,
            r: 13.0,
            mass: 1.5,
            ..Body::default()
        },
        hp: 3.0,
        angle: 0.0,
        orbit: 0.0,
        fire_t: 0.0,
        flash: 0.0,
        dead: false,
        next_target: 0,
    });
    let mut aims = Vec::new();
    for _ in 0..2 {
        w.drones[0].fire_t = 0.0;
        w.step_all(&[Input::default(), Input::default()], STEP);
        aims.push(w.ebullets.last().expect("a volley").vx.signum());
    }
    assert_eq!(
        aims,
        vec![-1.0, 1.0],
        "first at pilot 0 (west), then pilot 1 (east)"
    );
}

/// Events say which pilot they concern: a hit on pilot 1's tug is pilot
/// 1's alarm, while the shift clock's warnings are for everyone.
#[test]
fn events_are_addressed_to_the_pilot_they_concern() {
    let mut w = two_pilot_field();
    w.pilots[1].ship.invuln = 0.0;
    w.damage_ship(1, 50.0);
    assert!(!w.events.is_empty());
    assert!(
        w.events.iter().all(|n| n.to == Some(1)),
        "all about pilot 1"
    );
    w.events.clear();
    w.time_left = 60.5;
    for _ in 0..40 {
        w.step_all(&[Input::default(), Input::default()], STEP);
    }
    let minute = w
        .events
        .iter()
        .find(|n| {
            matches!(
                n.event,
                Event::Radio {
                    key: cj_sim::events::Radio::Minute,
                    ..
                }
            )
        })
        .expect("the one-minute warning");
    assert_eq!(minute.to, None, "the clock is everyone's");
}

/// A field sized for three pilots has twice a solo field's area and is
/// seeded to the same density: twice the loose salvage. A solo field is
/// exactly the solo field.
#[test]
fn a_bigger_field_is_seeded_to_the_same_density() {
    let mut solo = World::empty(Rng::Constant(0.5), VIEW);
    assert_eq!(
        (solo.edge_lo, solo.edge_hi, solo.area()),
        (0.0, WORLD_W, 1.0)
    );
    solo.populate();
    assert_eq!(solo.items.len(), START_ITEMS);

    let mut three = World::empty(Rng::Constant(0.5), VIEW);
    three.size_for(3);
    assert!((three.area() - 2.0).abs() < 1e-9);
    three.populate();
    assert_eq!(three.items.len(), START_ITEMS * 2);
}

/// Twenty pilots get 10.5 times a solo field's area, about 3.2 times as
/// wide, grown evenly so the carrier stays at the centre.
#[test]
fn a_twenty_pilot_field_is_three_times_as_wide_around_the_carrier() {
    let mut w = World::empty(Rng::Constant(0.5), VIEW);
    w.size_for(20);
    let width = w.edge_hi - w.edge_lo;
    assert!((width / WORLD_W - 10.5f64.sqrt()).abs() < 1e-9, "{width}");
    assert!(((w.edge_lo + w.edge_hi) / 2.0 - CARRIER_X).abs() < 1e-9);
}

/// With nothing else interfering, predicting a tug towing cargo lands
/// exactly where the server's full step puts it.
#[test]
fn prediction_matches_the_server_when_nothing_interferes() {
    let fresh = || {
        let mut w = field();
        add(&mut w, ItemType::Scrap, 1100.0, 1000.0);
        w.action(Action::Hook);
        w
    };
    let (mut server, mut client) = (fresh(), fresh());
    for t in 0..240 {
        let input = Input {
            thrust: 0.8,
            turn: (f64::from(t) * STEP).sin(),
            ..Input::default()
        };
        server.step(&input, STEP);
        client.predict_step(0, &input, STEP);
    }
    assert_eq!(server.pilots[0].ship.b, client.pilots[0].ship.b, "the tug");
    assert_eq!(server.pilots[0].ship.angle, client.pilots[0].ship.angle);
    assert_eq!(server.items[0].b, client.items[0].b, "its cargo");
}

/// Predicted contacts push but never deal damage: that is the server's call.
#[test]
fn predicted_contacts_deal_no_damage() {
    let mut w = field();
    w.pilots[0].ship.invuln = 0.0;
    w.pilots[0].ship.shield = 0.0;
    let rock = cj_sim::entities::make_rock(&mut w.rng, 3, 1040.0, 1000.0, 0.0, 0.0, false);
    w.rocks.push(rock);
    let hp = w.pilots[0].ship.hp;
    w.pilots[0].ship.b.vx = 600.0;
    for _ in 0..10 {
        w.predict_step(0, &Input::default(), STEP);
    }
    assert_eq!(w.pilots[0].ship.hp, hp);
    assert!(w.pilots[0].ship.b.vx < 600.0, "the rock pushed back");
}

/// Puts pilot `p`'s tug in a collector's net, already dragged past the
/// field's left edge, so the next step impounds it.
fn netted_past_the_edge(w: &mut World, p: usize) {
    w.spawn_collector(p);
    let c = w.collectors.len() - 1;
    let id = w.collectors[c].id;
    w.collectors[c].state = cj_sim::opposition::CollectorState::Tow;
    w.collectors[c].exit = (-1.0, 0.0);
    w.pilots[p].ship.net = Some(cj_sim::opposition::NetHold { by: id, hp: 100.0 });
    w.pilots[p].ship.b.x = w.edge_lo - 100.0;
    // The collector tows from just ahead, so the tether doesn't pull the
    // tug back in.
    w.collectors[c].b.x = w.edge_lo - 200.0;
    w.collectors[c].b.y = w.pilots[p].ship.b.y;
    w.collectors[c].tether = 100.0;
}

/// Alone in the field, an impounded tug ends the shift.
#[test]
fn an_impound_ends_a_single_player_shift() {
    let mut w = field();
    netted_past_the_edge(&mut w, 0);
    w.step(&Input::default(), STEP);
    assert!(w.pilots[0].impounded);
    assert!(w.shift_over);
}

/// Regression: one pilot's impound ended the shift for everyone. Now that
/// tug sits out the rest of the shift (no respawn) while the others fly on,
/// and the shift ends early only once every tug is impounded.
#[test]
fn an_impound_takes_out_only_that_pilots_tug() {
    let mut w = two_pilot_field();
    let inputs = [Input::default(), Input::default()];
    netted_past_the_edge(&mut w, 0);
    let left = w.time_left;
    w.step_all(&inputs, STEP);
    assert!(w.pilots[0].impounded);
    assert!(!w.shift_over, "the other pilot's shift goes on");
    for _ in 0..(RESPAWN_TIME / STEP) as usize + 60 {
        w.step_all(&inputs, STEP);
    }
    assert!(!w.pilots[0].ship.alive, "an impounded tug never respawns");
    assert!(w.pilots[1].ship.alive);
    assert!(w.time_left < left - RESPAWN_TIME, "the clock runs on");

    netted_past_the_edge(&mut w, 1);
    w.step_all(&inputs, STEP);
    assert!(w.shift_over, "nobody is left flying");
}

/// Regression: arresting one pilot sent home every unit in the field,
/// including those chasing a different wanted pilot.
#[test]
fn an_arrest_stands_down_only_that_pilots_pursuers() {
    use cj_sim::opposition::SheriffState;
    let mut w = two_pilot_field();
    for p in 0..2 {
        let (x, y) = (w.pilots[p].ship.b.x, w.pilots[p].ship.b.y);
        w.add_wanted(p, 2, x, y);
        w.spawn_sheriff(p, true);
    }
    for sh in &mut w.sheriffs {
        sh.state = SheriffState::Pursuit;
    }
    assert_eq!((w.sheriffs[0].target, w.sheriffs[1].target), (0, 1));
    w.arrest(1);
    assert_eq!(w.pilots[1].wanted, 0);
    assert_eq!(w.pilots[0].wanted, 2, "still wanted");
    assert_eq!(
        w.sheriffs[0].state,
        SheriffState::Pursuit,
        "pilot 0 is still chased"
    );
    assert_eq!(w.sheriffs[1].state, SheriffState::Leave);
}
