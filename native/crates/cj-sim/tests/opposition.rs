//! The browser's own behaviour tests for the opposition and weapons
//! (tests/specs/game.spec.js and weapons.spec.js in the claim-jumpers-game repository),
//! ported to the Rust simulation. Each keeps the browser test's setup and
//! assertions; where the browser pins Math.random, so does this.

use cj_sim::career::{Career, Mode};
use cj_sim::consts::*;
use cj_sim::entities::Bullet;
use cj_sim::items::ItemType;
use cj_sim::opposition::{CollectorState, RefineryState, SheriffState, Tgt};
use cj_sim::parts::{Part, System};
use cj_sim::rng::Rng;
use cj_sim::{Action, Input, World};

const VIEW: (f64, f64) = (1280.0, 720.0);

/// startShift / quietShift: a career shift with no surprise rivals,
/// collectors or law.
fn quiet(c: &Career, rng: Rng) -> World {
    let mut w = World::for_career(rng, VIEW, c);
    w.jumpers.clear();
    w.collectors.clear();
    w.sheriffs.clear();
    w.jumper_timer = 1e9;
    w.patrol_t = 1e9;
    w.collector_queue.clear();
    w
}

fn step(w: &mut World, input: &Input) {
    w.step(input, STEP);
}

#[test]
fn debt_collectors_only_net_when_the_payment_is_at_risk() {
    let mut c = Career::new(Mode::Standard);
    c.shift = 3;
    let mut w = quiet(&c, Rng::seeded(11));
    let park = |w: &mut World| {
        let s = &mut w.pilots[0].ship;
        s.b.x = BAY_X + 1200.0;
        s.b.y = BAY_Y;
        s.b.vx = 0.0;
        s.b.vy = 0.0;
        s.hp = 999.0;
        w.time_left = 40.0;
    };
    w.pilots[0].career.as_mut().unwrap().wallet = 1e6;
    park(&mut w);
    w.spawn_collector(0);
    let mut calm = None;
    for _ in 0..40 * 60 {
        step(&mut w, &Input::default());
        park(&mut w);
        let c = &w.collectors[0];
        if c.state == CollectorState::Leave || !w.nets.is_empty() {
            calm = Some(c.state);
            break;
        }
    }
    assert_eq!(
        calm,
        Some(CollectorState::Leave),
        "a covered payment: it watches, then leaves"
    );

    w.collectors.clear();
    w.nets.clear();
    w.pilots[0].career.as_mut().unwrap().wallet = 0.0;
    w.pilots[0].banked = 0.0;
    w.spawn_collector(0);
    let netted = (0..30 * 60).any(|_| {
        step(&mut w, &Input::default());
        park(&mut w);
        !w.nets.is_empty() || w.collectors[0].state == CollectorState::Tow
    });
    assert!(netted, "an at-risk payment: it nets the tug");
}

#[test]
fn regression_a_shakedown_roll_starts_a_chase_in_the_same_frame() {
    // Math.random pinned at 0.001: the shakedown roll always succeeds.
    let mut w = quiet(&Career::new(Mode::Standard), Rng::Constant(0.001));
    w.pilots[0].ship.b.x = BAY_X + 1500.0;
    w.pilots[0].ship.b.y = BAY_Y;
    w.pilots[0].ship.angle = 0.0;
    let it = w.new_item(
        ItemType::Scrap,
        w.pilots[0].ship.b.x + 50.0,
        w.pilots[0].ship.b.y,
        0.0,
        0.0,
    );
    w.items.push(it);
    w.action(Action::Hook);
    w.spawn_sheriff(0, false);
    w.sheriffs[0].b.x = w.pilots[0].ship.b.x + 350.0;
    w.sheriffs[0].b.y = w.pilots[0].ship.b.y;
    for _ in 0..120 {
        if w.sheriffs[0].state == SheriffState::Pursuit {
            break;
        }
        step(&mut w, &Input::default());
    }
    let sh = &w.sheriffs[0];
    assert_eq!(sh.state, SheriffState::Pursuit);
    assert!(sh.shakedown);
    assert_eq!(w.pilots[0].wanted, 0);
}

#[test]
fn a_wrecked_tug_orphans_its_sentries_and_rogue_kills_are_not_blamed_on_you() {
    let mut w = quiet(&Career::new(Mode::Standard), Rng::seeded(5));
    w.pilots[0].ship.b.x = BAY_X + 1400.0;
    w.pilots[0].ship.b.y = BAY_Y;
    w.sentries.clear();
    for i in 0..3 {
        let s = w.make_sentry(0, i, 3);
        w.sentries.push(s);
    }
    let (x, y) = (w.pilots[0].ship.b.x + 300.0, w.pilots[0].ship.b.y + 100.0);
    let mut j = w.new_jumper(x, y, Some(Variant::Brute));
    j.hunter = false;
    let id = j.id;
    w.jumpers.push(j);
    let grudge = w.grudge;
    w.kill_ship(0);
    assert!(w.sentries.iter().all(|d| d.rogue));
    for _ in 0..10 * 60 {
        step(&mut w, &Input::default());
        let Some(k) = w.jumper_index(id) else { break };
        let jm = &mut w.jumpers[k];
        if jm.dead {
            break;
        }
        jm.b.x = x;
        jm.b.y = y;
        jm.b.vx = 0.0;
        jm.b.vy = 0.0;
    }
    assert!(
        w.jumper_index(id).is_none_or(|k| w.jumpers[k].dead),
        "the rogues killed it"
    );
    assert_eq!(w.grudge - grudge, 0.0, "not your kill: no grudge");
}

#[test]
fn auto_defense_missiles_ignore_collectors_and_the_law_until_you_hit_them() {
    let mut w = quiet(&Career::new(Mode::Standard), Rng::seeded(9));
    w.pilots[0].ship.b.x = BAY_X + 1500.0;
    w.pilots[0].ship.b.y = BAY_Y;
    w.spawn_collector(0);
    w.spawn_sheriff(0, false);
    let kinds = |w: &World| {
        let t = w.missile_targets();
        (
            t.iter().any(|t| matches!(t, Tgt::Collector(_))),
            t.iter().any(|t| matches!(t, Tgt::Sheriff(_))),
        )
    };
    assert_eq!(kinds(&w), (false, false));
    // A stray sentry bolt is not you shooting first.
    let c = w.collectors[0].b;
    w.bullets.push(Bullet {
        x: c.x - 3.0,
        y: c.y,
        vx: 800.0,
        life: 0.5,
        dmg: 0.1,
        sentry: true,
        ..Bullet::default()
    });
    step(&mut w, &Input::default());
    assert!(!kinds(&w).0);
    // Your own cannon is.
    let (c, sh) = (w.collectors[0].b, w.sheriffs[0].b);
    for b in [c, sh] {
        w.bullets.push(Bullet {
            x: b.x - 3.0,
            y: b.y,
            vx: 800.0,
            life: 0.5,
            dmg: 0.1,
            ..Bullet::default()
        });
    }
    step(&mut w, &Input::default());
    assert_eq!(kinds(&w), (true, true));
}

#[test]
fn a_wrecked_tug_clears_the_wanted_level_and_calls_off_the_law() {
    let mut w = quiet(&Career::new(Mode::Standard), Rng::seeded(13));
    w.pilots[0].ship.b.x = BAY_X + 1500.0;
    w.pilots[0].ship.b.y = BAY_Y;
    let (x, y) = (w.pilots[0].ship.b.x, w.pilots[0].ship.b.y);
    w.add_wanted(0, 4, x, y);
    w.reinforce_t = Some(0.0);
    for _ in 0..300 {
        step(&mut w, &Input::default());
        w.pilots[0].ship.hp = 999.0;
    }
    let units = w.sheriffs.iter().filter(|s| !s.dead).count();
    w.kill_ship(0);
    let leaving = w
        .sheriffs
        .iter()
        .filter(|s| !s.dead)
        .all(|s| s.state == SheriffState::Leave);
    for _ in 0..240 {
        step(&mut w, &Input::default()); // respawn
    }
    assert!(units > 0);
    assert_eq!(w.pilots[0].wanted, 0);
    assert!(leaving);
    assert!(!w.sheriffs.iter().any(|s| s.state == SheriffState::Pursuit));
}

#[test]
fn reaching_the_carrier_bay_clears_the_wanted_level() {
    let mut c = Career::new(Mode::Standard);
    c.shift = 3;
    let mut w = World::for_career(Rng::seeded(17), VIEW, &c);
    w.jumpers.clear();
    w.collectors.clear();
    w.jumper_timer = 1e9;
    w.collector_queue.clear();
    w.pilots[0].ship.b.x = BAY_X + 1500.0;
    w.pilots[0].ship.b.y = BAY_Y;
    let (x, y) = (w.pilots[0].ship.b.x, w.pilots[0].ship.b.y);
    w.add_wanted(0, 3, x, y);
    w.reinforce_t = Some(0.0);
    for _ in 0..240 {
        step(&mut w, &Input::default());
        w.pilots[0].ship.hp = 999.0;
    }
    let chasing = w.sheriffs.iter().any(|s| s.state == SheriffState::Pursuit);
    let s = &mut w.pilots[0].ship;
    s.b.x = BAY_X;
    s.b.y = BAY_Y;
    s.b.vx = 0.0;
    s.b.vy = 0.0;
    step(&mut w, &Input::default());
    assert!(chasing);
    assert_eq!(w.pilots[0].wanted, 0);
    assert!(w
        .sheriffs
        .iter()
        .filter(|s| !s.dead)
        .all(|s| s.state == SheriffState::Leave));
}

#[test]
fn cannon_levels_add_spread_shots_and_level_5_missiles_fire_torpedo_salvos() {
    let mut c = Career::new(Mode::Standard);
    c.shift = 3;
    c.owned.push(Part::Missiles);
    c.mounted[1] = Some(Part::Missiles);
    c.levels.set(System::Cannon, 4);
    c.levels.set(System::Missiles, 5);
    let mut w = quiet(&c, Rng::seeded(21));
    w.pilots[0].ship.b.x = BAY_X + 1500.0;
    w.pilots[0].ship.b.y = BAY_Y + 800.0;
    w.pilots[0].ship.angle = 0.0;
    w.bullets.clear();
    w.pilots[0].ship.fire_t = 0.0;
    w.pilots[0].ship.heat = 0.0;
    let fire = Input {
        fire: true,
        ..Input::default()
    };
    step(&mut w, &fire);
    let shots = w.bullets.len();
    for i in 0..3 {
        let (x, y) = (
            w.pilots[0].ship.b.x + 300.0 + f64::from(i) * 60.0,
            w.pilots[0].ship.b.y + 80.0 * f64::from(i),
        );
        let mut j = w.new_jumper(x, y, Some(Variant::Cutter));
        j.hunter = false;
        w.jumpers.push(j);
    }
    w.pilots[0].ship.missile_gap_t = 0.0;
    w.missiles.clear();
    w.update_missiles(STEP);
    assert_eq!(shots, 5);
    assert_eq!(w.missiles.len(), 2, "a salvo of two");
    assert!(w.missiles.iter().all(|m| m.torpedo));
    assert_eq!(w.pilots[0].mods.missile_rack, 6);
}

#[test]
fn the_afterburner_boosts_speed_and_chained_bursts_overheat_the_engine() {
    let mut c = Career::new(Mode::Standard);
    c.shift = 3;
    let mut w = quiet(&c, Rng::seeded(23));
    let park = |w: &mut World| {
        let s = &mut w.pilots[0].ship;
        s.b.x = BAY_X + 1500.0;
        s.b.y = BAY_Y + 800.0;
        s.b.vx = 0.0;
        s.b.vy = 0.0;
        s.invuln = 99.0;
        s.hp = 999.0;
    };
    let speed = |w: &mut World, burn: bool| {
        park(w);
        let s = &mut w.pilots[0].ship;
        s.angle = 0.0;
        s.burn_t = 0.0;
        s.engine_heat = 0.0;
        s.engine_over_t = 0.0;
        s.prev_burn = false;
        let input = Input {
            thrust: 1.0,
            burn,
            ..Input::default()
        };
        for _ in 0..60 {
            step(w, &input);
            w.pilots[0].ship.invuln = 99.0;
        }
        w.pilots[0].ship.b.speed()
    };
    let normal = speed(&mut w, false);
    let burned = speed(&mut w, true);
    assert!(burned > normal * 1.4, "{burned} vs {normal}");

    park(&mut w);
    let s = &mut w.pilots[0].ship;
    s.burn_t = 0.0;
    s.engine_heat = 0.0;
    s.engine_over_t = 0.0;
    s.prev_burn = false;
    let mut overheated_on = None;
    for b in 1..=4 {
        step(
            &mut w,
            &Input {
                burn: true,
                ..Input::default()
            },
        );
        if w.pilots[0].ship.engine_over_t > 0.0 {
            overheated_on = Some(b);
            break;
        }
        for _ in 0..185 {
            step(&mut w, &Input::default());
            park(&mut w);
        }
    }
    assert_eq!(overheated_on, Some(3));
}

#[test]
fn the_refinery_jaws_pull_from_half_as_far_and_can_be_wrecked() {
    let mut c = Career::new(Mode::Standard);
    c.shift = 3;
    let mut w = quiet(&c, Rng::seeded(29));
    w.pilots[0].ship.b.x = BAY_X + 1500.0;
    w.pilots[0].ship.b.y = BAY_Y + 800.0;
    w.spawn_refinery();
    {
        let r = w.refinery.as_mut().unwrap();
        r.b.vx = 0.0;
        r.b.vy = 0.0;
        r.spin = 0.0;
    }
    let (px, py) = w.refinery.as_ref().unwrap().socket_pos(0);
    let it = w.new_item(ItemType::Charge, px + 70.0, py, 0.0, 0.0);
    w.items.push(it);
    step(&mut w, &Input::default());
    let seated = w.refinery.as_ref().unwrap().sockets[0].filled;
    {
        let r = w.refinery.as_mut().unwrap();
        r.state = RefineryState::Hungry;
        r.hunger = 1.0;
    }
    let (mx, my) = w.refinery.as_ref().unwrap().maw();
    let place_rock = |w: &mut World, off: f64| {
        let r = &mut w.rocks[0];
        r.b.x = mx + off;
        r.b.y = my;
        r.b.vx = 0.0;
        r.b.vy = 0.0;
        step(w, &Input::default());
        w.rocks[0].b.speed() > 0.001
    };
    let pulled_at_700 = place_rock(&mut w, 700.0);
    let pull_r = w.refinery.as_ref().unwrap().pull_r;
    w.hurt_jaws(999.0);
    let pulls_when_wrecked = place_rock(&mut w, 200.0);
    assert!(seated);
    assert_eq!(pull_r, 550.0);
    assert!(!pulled_at_700);
    assert_eq!(w.refinery.as_ref().unwrap().state, RefineryState::Wrecked);
    assert!(!pulls_when_wrecked);
}

#[test]
fn a_missile_blast_knocks_out_hostile_engines_in_range_for_ten_seconds() {
    use cj_sim::opposition::{Missile, EMP_TIME, MISSILE_EMP_R};
    let mut w = quiet(&Career::new(Mode::Standard), Rng::seeded(21));
    // No rocks to bump the drifting rivals: only their engines could speed them up.
    w.rocks.clear();
    w.pilots[0].ship.b.x = BAY_X + 1500.0;
    w.pilots[0].ship.b.y = BAY_Y;
    let (x, y) = (BAY_X + 2400.0, BAY_Y);
    // Struck, inside the pulse, and well outside it.
    for dx in [0.0, MISSILE_EMP_R * 0.8, MISSILE_EMP_R + 300.0] {
        let mut j = w.new_jumper(x + dx, y, Some(Variant::Brute));
        j.hunter = false;
        j.b.vx = 0.0;
        j.b.vy = 0.0;
        w.jumpers.push(j);
    }
    let id = w.jumpers[0].id;
    w.missiles.push(Missile {
        x,
        y,
        vx: 0.0,
        vy: 0.0,
        angle: 0.0,
        target: Some(Tgt::Jumper(id)),
        life: 1.0,
        torpedo: false,
        pilot: 0,
        tracking: 1.0,
    });
    step(&mut w, &Input::default());
    let emp: Vec<f64> = w.jumpers.iter().map(|j| j.emp_t).collect();
    assert!(
        emp[0] > EMP_TIME - 0.1 && emp[1] > EMP_TIME - 0.1,
        "{emp:?}"
    );
    assert_eq!(emp[2], 0.0);
    // A disabled rival keeps its drift: it can't accelerate.
    let v0 = js_speed(&w, 1);
    for _ in 0..(5.0 / STEP) as usize {
        step(&mut w, &Input::default());
        assert!(js_speed(&w, 1) <= v0 + 1e-6, "engine fired while EMP'd");
    }
    // After the ten seconds it flies again.
    for _ in 0..(6.0 / STEP) as usize {
        step(&mut w, &Input::default());
    }
    assert_eq!(w.jumpers[1].emp_t, 0.0);
    for _ in 0..(2.0 / STEP) as usize {
        step(&mut w, &Input::default());
    }
    assert!(js_speed(&w, 1) > v0 + 1.0, "engines never came back");
}

fn js_speed(w: &World, j: usize) -> f64 {
    let b = w.jumpers[j].b;
    b.vx.hypot(b.vy)
}
