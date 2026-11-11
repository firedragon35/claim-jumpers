//! The career rules against the browser's own answers, recorded by
//! native/tools/golden/career.mjs into golden/career.json: loadout numbers
//! (computeMods), installments and overdraft limits, repairing saved
//! careers on load (loadCareer), and settling a shift (endShift).
//! Plus save round-trips and the shop's rules.

use cj_sim::career::{installment_for, Career, Mode, Outcome};
use cj_sim::field::{chain_contract, Belt, Chain, Condition, Contract, ContractKind, Crew};
use cj_sim::items::ItemType;
use cj_sim::mods::compute_mods;
use cj_sim::parts::*;
use cj_sim::rng::Rng;
use cj_sim::World;
use serde_json::Value;

fn golden() -> Value {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/career.json");
    serde_json::from_str(&std::fs::read_to_string(p).expect("career.json")).expect("JSON")
}

fn mounted_of(v: &Value) -> Mounted {
    let mut m: Mounted = [None; 5];
    for (i, s) in SLOTS.iter().enumerate() {
        m[i] = v[s.key].as_str().and_then(Part::from_key);
    }
    m
}

fn levels_of(v: &Value) -> Levels {
    let mut l = Levels::default();
    for s in SYSTEMS {
        l.set(s, v[s.key()].as_u64().unwrap_or(1) as u8);
    }
    l
}

fn close(a: f64, b: f64, what: &str) {
    assert!((a - b).abs() < 1e-9, "{what}: browser {b}, rust {a}");
}

#[test]
fn loadout_numbers_match_compute_mods() {
    for case in golden()["mods"].as_array().unwrap() {
        let m = compute_mods(
            &mounted_of(&case["mounted"]),
            &levels_of(&case["levels"]),
            case["tugClass"].as_u64().unwrap() as usize,
        );
        let want = &case["mods"];
        let f = |k: &str| want[k].as_f64().unwrap();
        let b = |k: &str| want[k].as_bool().unwrap();
        let ctx = format!("{case}");
        for (got, key) in [
            (m.winch_cap, "winchCap"),
            (m.reel, "reel"),
            (m.hook_range, "hookRange"),
            (m.max_hp, "maxHp"),
            (m.mass, "mass"),
            (m.turn_load, "turnLoad"),
            (m.thrust_mul, "thrustMul"),
            (m.turn_mul, "turnMul"),
            (m.shield_max, "shieldMax"),
            (m.shield_rate, "shieldRate"),
            (m.shield_delay, "shieldDelay"),
            (m.cannon_dmg, "cannonDmg"),
            (m.fire_mul, "fireMul"),
            (m.heat_mul, "heatMul"),
            (f64::from(m.cannon_spread), "cannonSpread"),
            (m.strafe_mul, "strafeMul"),
            (m.strafe_heat_mul, "strafeHeatMul"),
            (f64::from(m.missile_rack), "missileRack"),
            (m.missile_reload, "missileReload"),
            (m.missile_dmg, "missileDmg"),
            (f64::from(m.missile_salvo), "missileSalvo"),
            (m.drone_hp, "droneHp"),
            (m.drone_dmg, "droneDmg"),
            (m.drone_fire, "droneFire"),
        ] {
            close(got, f(key), &format!("{key} in {ctx}"));
        }
        for (got, key) in [
            (m.thrusters, "thrusters"),
            (m.scoop, "scoop"),
            (m.grinder, "grinder"),
            (m.armor, "armor"),
            (m.missiles, "missiles"),
            (m.drone_bay, "droneBay"),
            (m.torpedo, "torpedo"),
        ] {
            assert_eq!(got, b(key), "{key} in {ctx}");
        }
        let barrels: Vec<f64> = want["barrels"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap())
            .collect();
        assert_eq!(m.barrels, barrels, "barrels in {ctx}");
    }
}

#[test]
fn installments_and_dues_match() {
    let g = golden();
    for i in g["installments"].as_array().unwrap() {
        let mode = Mode::from_key(i["mode"].as_str().unwrap()).unwrap();
        let got = installment_for(
            i["n"].as_i64().unwrap() as i32,
            i["loan"].as_f64().unwrap(),
            mode,
        );
        close(got, i["due"].as_f64().unwrap(), &format!("installment {i}"));
    }
    for d in g["dues"].as_array().unwrap() {
        let mut c = Career::new(Mode::Hard);
        c.shift = 7;
        c.loan_start = 3;
        c.loan = 55555.0;
        c.rep.bank = d["bank"].as_f64().unwrap();
        close(
            c.due(d["ahead"].as_i64().unwrap() as i32),
            d["due"].as_f64().unwrap(),
            &format!("due {d}"),
        );
        close(c.overdraft_limit(), d["limit"].as_f64().unwrap(), "limit");
    }
}

/// The ported fields of a career, as the browser writes them.
fn check_career(c: &Career, want: &Value, ctx: &str) {
    let f = |k: &str| want[k].as_f64().unwrap();
    close(f64::from(c.shift), f("shift"), &format!("shift {ctx}"));
    close(c.wallet, f("wallet"), &format!("wallet {ctx}"));
    close(c.loan, f("loan"), &format!("loan {ctx}"));
    close(c.grudge, f("grudge"), &format!("grudge {ctx}"));
    close(
        c.tug_class as f64,
        f("tugClass"),
        &format!("tugClass {ctx}"),
    );
    close(
        f64::from(c.loan_start),
        f("loanStart"),
        &format!("loanStart {ctx}"),
    );
    close(
        c.rep.bank,
        want["rep"]["bank"].as_f64().unwrap(),
        &format!("rep.bank {ctx}"),
    );
    close(
        c.rep.law,
        want["rep"]["law"].as_f64().unwrap(),
        &format!("rep.law {ctx}"),
    );
    close(
        c.rep.rivals,
        want["rep"]["rivals"].as_f64().unwrap(),
        &format!("rep.rivals {ctx}"),
    );
}

/// The shift's contract as the exporter gave it: a chain step, or a
/// plain contract object.
fn contract_input(input: &Value) -> Option<Contract> {
    if let Some(step) = input["chainStep"].as_array() {
        let ch = Chain::from_key(step[0].as_str().unwrap()).expect("chain");
        return Some(chain_contract(ch, step[1].as_u64().unwrap() as usize));
    }
    let k = input.get("contract")?;
    Some(Contract {
        kind: ContractKind::from_key(k["kind"].as_str().unwrap()).expect("kind"),
        n: k["n"].as_f64().unwrap(),
        cargo: k["type"].as_str().and_then(ItemType::from_key),
        reward: k["reward"].as_f64().unwrap(),
        chain: None,
    })
}

/// Crew, cores, chains and the unique parts a finished chain awards.
fn check_field_state(c: &Career, want: &Value, ctx: &str) {
    assert_eq!(
        c.crew.map(|x| x.def().key),
        want["crew"].as_str(),
        "crew {ctx}"
    );
    close(
        f64::from(c.cores),
        want["cores"].as_f64().unwrap_or(0.0),
        &format!("cores {ctx}"),
    );
    let chain = want["chain"]["id"].as_str().map(|id| {
        let step = want["chain"]["step"].as_u64().unwrap() as usize;
        (Chain::from_key(id).expect("chain"), step)
    });
    assert_eq!(c.chain, chain, "chain {ctx}");
    let done: Vec<Chain> = want["chainsDone"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().and_then(Chain::from_key))
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(c.chains_done, done, "chainsDone {ctx}");
    let owned: Vec<&str> = c.owned.iter().map(|p| p.def().key).collect();
    let want_owned: Vec<&str> = want["owned"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(owned, want_owned, "owned {ctx}");
    assert!(c.contract.is_none(), "the contract is spent {ctx}");
}

#[test]
fn malformed_saves_are_repaired_like_the_browser() {
    for l in golden()["loads"].as_array().unwrap() {
        let c = Career::from_json(&l["raw"].to_string()).expect("an object loads");
        let want = &l["career"];
        let ctx = l["raw"].to_string();
        check_career(&c, want, &ctx);
        close(
            c.total_banked,
            want["totalBanked"].as_f64().unwrap(),
            "totalBanked",
        );
        assert_eq!(
            c.mode.def().key,
            want["difficulty"].as_str().unwrap(),
            "{ctx}"
        );
        assert_eq!(
            c.sentries as u64,
            want["sentries"].as_u64().unwrap(),
            "{ctx}"
        );
        let owned: Vec<&str> = c.owned.iter().map(|p| p.def().key).collect();
        let want_owned: Vec<&str> = want["owned"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(owned, want_owned, "{ctx}");
        assert_eq!(c.mounted, mounted_of(&want["mounted"]), "{ctx}");
        assert_eq!(c.levels, levels_of(&want["levels"]), "{ctx}");
        if let Some(ls) = want.get("lastShift").and_then(Value::as_object) {
            let got = c.extra["lastShift"].as_object().expect("lastShift kept");
            assert_eq!(got.len(), ls.len(), "{ctx}");
            for (k, v) in ls {
                // Numbers compare by value (the browser writes 3, Rust 3.0).
                match v.as_f64() {
                    Some(n) => close(got[k].as_f64().unwrap(), n, &format!("lastShift.{k}")),
                    None => assert_eq!(&got[k], v, "lastShift.{k} {ctx}"),
                }
            }
        }
    }
}

#[test]
fn shift_settlements_match_end_shift() {
    for s in golden()["settles"].as_array().unwrap() {
        let input = &s["input"];
        let k = &input["career"];
        let mode = k["difficulty"]
            .as_str()
            .and_then(Mode::from_key)
            .unwrap_or(Mode::Standard);
        let mut c = Career::new(mode);
        c.shift = k["shift"].as_i64().unwrap() as i32;
        c.wallet = k["wallet"].as_f64().unwrap_or(0.0);
        c.loan = k["loan"].as_f64().unwrap_or(c.loan);
        c.tug_class = k["tugClass"].as_u64().unwrap_or(0) as usize;
        c.loan_start = k["loanStart"].as_i64().unwrap_or(1) as i32;
        if let Some(r) = k.get("rep") {
            c.rep.bank = r["bank"].as_f64().unwrap();
            c.rep.law = r["law"].as_f64().unwrap();
            c.rep.rivals = r["rivals"].as_f64().unwrap();
        }
        // Phase 6 and 7 inputs: the crew, an open chain, stored cores.
        c.crew = k["crew"].as_str().and_then(Crew::from_key);
        c.cores = k["cores"].as_u64().unwrap_or(0) as u32;
        c.chain = k["chain"]["id"]
            .as_str()
            .and_then(Chain::from_key)
            .map(|ch| (ch, k["chain"]["step"].as_u64().unwrap() as usize));
        let mut w = World::for_career(Rng::Constant(0.5), (1280.0, 720.0), &c);
        w.pilots[0].contract = contract_input(input);
        if let Some(d) = input["delivered"].as_object() {
            for (t, n) in d {
                let t = ItemType::from_key(t).expect("item type");
                w.pilots[0].delivered.push((t, n.as_u64().unwrap() as u32));
            }
        }
        w.pilots[0].stats.deaths = input["deaths"].as_u64().unwrap_or(0) as u32;
        w.pilots[0].cores_banked = input["coresBanked"].as_u64().unwrap_or(0) as u32;
        w.pilots[0].banked = input["banked"].as_f64().unwrap();
        w.grudge = input["grudge"].as_f64().unwrap();
        w.pilots[0].stats.jumpers = input["jumpers"].as_u64().unwrap() as u32;
        w.pilots[0].rep_delta.rivals = input["rivalsDelta"].as_f64().unwrap();
        let st = c.settle(&w, 0);
        let title = s["title"].as_str().unwrap();
        let ctx = format!("{input} -> {title}");
        let want = &s["career"];
        check_career(&c, want, &ctx);
        if want.is_object() {
            check_field_state(&c, want, &ctx);
        }
        let want_outcome = if title == "Repossessed" {
            "repo"
        } else if title.starts_with("Traded up") {
            "trade"
        } else if title == "Fleet paid off" {
            "fleet"
        } else {
            "ok"
        };
        let got = match st.outcome {
            Outcome::Continue => "ok",
            Outcome::Repossessed { .. } => "repo",
            Outcome::TradedUp { .. } => "trade",
            Outcome::FleetPaid => "fleet",
        };
        assert_eq!(got, want_outcome, "{ctx}");
        // The browser keeps the save unless the career ended.
        assert_eq!(
            s["saved"].as_bool().unwrap(),
            matches!(got, "ok" | "trade"),
            "{ctx}"
        );
    }
}

#[test]
fn save_round_trips_and_keeps_unported_fields() {
    let mut c = Career::new(Mode::Hard);
    c.wallet = -1234.5;
    c.shift = 6;
    c.owned.push(Part::Winch);
    c.mounted[3] = Some(Part::Winch);
    c.levels.set(System::Shield, 3);
    c.rep.bank = 42.0;
    // Phase 6 and 7 fields round-trip as fields (a Kessler permit needs a
    // Mk II tug, or the load would fall back to Hollis).
    c.tug_class = 1;
    c.belt = Belt::Kessler;
    c.crew = Some(Crew::Broker);
    c.contract = Some(Contract {
        kind: ContractKind::Deliver,
        n: 3.0,
        cargo: Some(cj_sim::items::ItemType::Plate),
        reward: 1500.0,
        chain: None,
    });
    c.contract_shift = 6;
    c.chain = Some((Chain::SaltyDog, 1));
    c.chains_done.push(Chain::HollisFreight);
    c.conditions = Some((
        6,
        [
            Condition::Normal,
            Condition::OreBoom,
            Condition::Night,
            Condition::IonFront,
            Condition::SalvageGlut,
        ],
    ));
    c.cores = 2;
    // A save key this build doesn't use is kept verbatim.
    c.extra.insert("records".into(), Value::from("kept"));
    let back = Career::from_json(&c.to_json()).expect("loads");
    assert_eq!(back, c);
    assert!(Career::from_json("not json").is_none());
    assert!(Career::from_json("[1, 2]").is_none());
}

#[test]
fn shop_buys_mounts_and_refuses_sensibly() {
    let mut c = Career::new(Mode::Standard);
    assert!(c.buy_part(Part::Winch).is_err(), "no money yet");
    c.wallet = 10000.0;
    assert!(c
        .buy_part(Part::Winch)
        .unwrap()
        .contains("bought and mounted"));
    assert!(c
        .buy_part(Part::Thrusters)
        .unwrap()
        .contains("bought and mounted"));
    // Both utility hardpoints are full now.
    let msg = c.buy_part(Part::Grinder).unwrap();
    assert!(msg.contains("Free a utility hardpoint"), "{msg}");
    assert!(c.mount(Part::Grinder).is_err());
    c.unmount(Part::Winch).unwrap();
    c.mount(Part::Grinder).unwrap();
    assert!(c.mods().grinder && !c.mods().winch_cap.eq(&26.0));
    assert_eq!(c.wallet, 10000.0 - 2800.0 - 2000.0 - 1500.0);
    // Unique chain parts can't be bought.
    assert!(c.buy_part(Part::Grapple).is_err());
    assert!(c.buy_part(Part::HullSeal).is_err());
    // Weapon systems need their weapon; levels stop at the max.
    assert_eq!(c.level_price(System::Missiles), None);
    c.wallet = 1e6;
    for _ in 0..3 {
        c.buy_level(System::Shield).unwrap();
    }
    assert!(c.buy_level(System::Shield).is_err());
    assert_eq!(c.mods().shield_max, 100.0);
    // Unmounting the cannon leaves the tug unarmed, and says so.
    assert!(c.unmount(Part::Cannon).unwrap().contains("No guns mounted"));
}

#[test]
fn paying_the_loan_off_trades_up() {
    let mut c = Career::new(Mode::Standard);
    c.wallet = 50000.0;
    let (_, outcome) = c.pay_extra(1e9).unwrap();
    assert_eq!(
        outcome,
        Some(Outcome::TradedUp {
            from: 0,
            rolled: 0.0
        })
    );
    assert_eq!(c.tug_class, 1);
    assert_eq!(c.loan, 60000.0);
    assert_eq!(c.wallet, 10000.0);
    assert_eq!(c.rep.bank, 80.0, "40 thousands at 2 each");
    // A partial payment is just a payment.
    let (msg, outcome) = c.pay_extra(1000.0).unwrap();
    assert!(outcome.is_none() && msg.contains("Paid 1,000 cr"));
}

#[test]
fn sentries_need_a_mounted_drone_bay_with_room() {
    let mut c = Career::new(Mode::Standard);
    c.wallet = 1e5;
    assert!(c.buy_sentry().is_err(), "no drone bay");
    c.buy_part(Part::DroneBay).unwrap();
    for _ in 0..SENTRY_MAX {
        c.buy_sentry().unwrap();
    }
    assert!(c.buy_sentry().is_err(), "the bay is full");
    assert_eq!(c.sentries, SENTRY_MAX);
    // A shift with the bay mounted launches them all.
    let w = World::for_career(Rng::seeded(1), (1280.0, 720.0), &c);
    assert_eq!(w.sentries.len(), SENTRY_MAX as usize);
}

#[test]
fn offers_survive_a_reload_and_one_bad_entry_drops_the_set() {
    let mut c = Career::new(Mode::Standard);
    c.shift = 4;
    c.ensure_offers(&mut Rng::seeded(3));
    let back = Career::from_json(&c.to_json()).expect("loads");
    assert_eq!(back.offers, c.offers);
    assert_eq!(back.offers_shift, 4);

    let mut raw: Value = serde_json::from_str(&c.to_json()).unwrap();
    raw["offers"][1] = serde_json::json!({ "kind": "nope" });
    let bad = Career::from_json(&raw.to_string()).expect("loads");
    assert!(bad.offers.is_empty(), "the shop rolls a new set");
}

#[test]
fn a_chain_step_and_a_matching_ordinary_offer_are_different_jobs() {
    let mut c = Career::new(Mode::Standard);
    let chain = chain_contract(Chain::HollisFreight, 0);
    let mut ordinary = chain.clone();
    ordinary.chain = None;
    c.offers = vec![chain, ordinary.clone()];
    c.contract = Some(ordinary);
    c.contract_shift = c.shift;
    assert!(!c.offer_taken(0));
    assert!(c.offer_taken(1));
}

#[test]
fn the_crews_adapt_to_a_dominant_habit_and_it_saves() {
    use cj_sim::career::{Habit, Habits};
    let few = Habits {
        cannon: 5.0,
        ..Habits::default()
    };
    assert_eq!(few.dominant(), None, "not enough kills to notice");
    let mixed = Habits {
        cannon: 4.0,
        smash: 4.0,
        missile: 4.0,
    };
    assert_eq!(mixed.dominant(), None, "no clear habit");
    let mut c = Career::new(Mode::Standard);
    c.habits.smash = 10.0;
    assert_eq!(c.habits.dominant(), Some(Habit::Smash));
    let back = Career::from_json(&c.to_json()).expect("loads");
    assert_eq!(back.habits, c.habits);
    let w = World::for_career(Rng::Constant(0.5), (1280.0, 720.0), &c);
    assert_eq!(w.rival_counter, Some(Habit::Smash));
}

/// Settle an empty shift, after `tweak` has set up what happened in it.
fn settled(c: &mut Career, tweak: impl FnOnce(&mut World)) {
    let mut w = World::for_career(Rng::Constant(0.5), (1280.0, 720.0), c);
    w.pilots[0].banked = 100_000.0; // never repossessed mid-test
    tweak(&mut w);
    c.settle(&w, 0);
}

#[test]
fn rival_standing_fades_unless_the_shift_pushed_it_further_out() {
    let after = |start: f64, kills: u32| {
        let mut c = Career::new(Mode::Standard);
        c.rep.rivals = start;
        settled(&mut c, |w| {
            w.pilots[0].stats.jumpers = kills;
            w.pilots[0].rep_delta.rivals = -3.0 * f64::from(kills);
        });
        c.rep.rivals
    };
    assert_eq!(
        after(-50.0, 0),
        -43.0,
        "+2 for a clean shift, then the fade"
    );
    assert_eq!(after(-50.0, 1), -53.0, "still feeding the feud");
    assert_eq!(after(50.0, 0), 52.0, "clean shifts hold good standing");
    assert_eq!(after(50.0, 1), 42.0, "kills and the fade together");
    assert_eq!(after(0.0, 0), 2.0);
}

#[test]
fn rival_standing_recovers_in_about_seven_clean_shifts_and_can_reach_zero() {
    let mut c = Career::new(Mode::Standard);
    c.rep.rivals = -100.0;
    for _ in 0..7 {
        settled(&mut c, |_| {});
    }
    assert!(c.rep.rivals > -40.0, "{}", c.rep.rivals);
    c.rep.rivals = -1.0;
    settled(&mut c, |w| w.pilots[0].stats.jumpers = 1);
    assert_eq!(c.rep.rivals, 0.0);
}

#[test]
fn stolen_cargo_buys_the_crews_upgrades() {
    let mut c = Career::new(Mode::Standard);
    let plain =
        World::for_career(Rng::Constant(0.5), (1280.0, 720.0), &c).new_jumper(0.0, 0.0, None);
    settled(&mut c, |w| w.rival_loot = 3500.0);
    assert_eq!(c.crew_level(), 1);
    let mut w = World::for_career(Rng::Constant(0.5), (1280.0, 720.0), &c);
    assert_eq!(w.new_jumper(0.0, 0.0, None).hp, plain.hp + 1.0);

    settled(&mut c, |w| w.rival_loot = 7000.0);
    assert_eq!(c.crew_level(), 3);
    let back = Career::from_json(&c.to_json()).expect("loads");
    assert_eq!(back.war_chest, c.war_chest);

    c.mode = Mode::Relaxed;
    assert_eq!(c.crew_level(), 2, "capped on Relaxed");
    c.mode = Mode::Standard;

    settled(&mut c, |w| w.boss_down = Some("Lucky Teodor"));
    assert_eq!(c.war_chest, 5250.0, "a boss kill empties half the chest");
    assert_eq!(c.crew_level(), 1);
}

#[test]
fn the_report_says_when_the_crews_upgrade() {
    let mut c = Career::new(Mode::Standard);
    let mut w = World::for_career(Rng::Constant(0.5), (1280.0, 720.0), &c);
    w.pilots[0].banked = 100_000.0;
    w.rival_loot = 3000.0;
    assert!(c.settle(&w, 0).chain_note.contains("tougher hulls"));
}

/// A shared server field takes its belt, conditions, difficulty and shift
/// number from the server, and each pilot's tug, difficulty and crew from
/// that player's own career; a shift there settles into each career apart.
#[test]
fn a_shared_field_flies_each_career_and_settles_each_apart() {
    use cj_sim::world::SharedShift;
    let mut rich = Career::new(Mode::Hard);
    rich.wallet = 50_000.0;
    rich.buy_level(System::Thrust).expect("affordable");
    rich.crew = Some(Crew::Rigger);
    let fresh = Career::new(Mode::Hard);
    let shared = SharedShift {
        belt: Belt::Hollis,
        cond: Condition::Night,
        mode: Mode::Hard,
        shift_no: 3,
        grudge: 0.0,
        pvp: false,
    };
    let careers = [rich.clone(), fresh.clone()];
    let mut w = World::for_careers(Rng::seeded(5), (1280.0, 720.0), 4, &shared, &careers);
    assert_eq!(w.pilots.len(), 2);
    assert_eq!(
        (w.belt, w.cond, w.shift_no),
        (Belt::Hollis, Condition::Night, 3)
    );
    assert!(
        w.pilots[0].mods.thrust_mul > w.pilots[1].mods.thrust_mul,
        "each tug is its own"
    );
    assert!(
        w.pilots[0].mods.hook_range > w.pilots[1].mods.hook_range,
        "the rigger"
    );
    assert_eq!(w.pilots[1].diff_damage, Mode::Hard.def().damage);
    let (a, b) = (w.pilots[0].ship.b, w.pilots[1].ship.b);
    assert!((a.x, a.y) != (b.x, b.y), "parked apart");

    let late = w.add_career_pilot(&fresh);
    assert_eq!(late, 2);
    assert!(w.pilots[2].career.is_some());

    w.pilots[1].banked = 1200.0;
    let mut one = rich.clone();
    let mut two = fresh.clone();
    let (s1, s2) = (one.settle(&w, 0), two.settle(&w, 1));
    assert_eq!(s1.earned, 0.0);
    assert!(s2.earned > 0.0, "the second pilot's haul is theirs");
    assert_eq!((one.shift, two.shift), (rich.shift + 1, fresh.shift + 1));
}
