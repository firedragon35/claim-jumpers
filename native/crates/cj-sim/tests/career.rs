//! The career rules against the browser's own answers, recorded by
//! native/tools/golden/career.mjs into golden/career.json: loadout numbers
//! (computeMods), installments and overdraft limits, repairing saved
//! careers on load (loadCareer), and settling a shift (endShift).
//! Plus save round-trips and the shop's rules.

use cj_sim::career::{installment_for, Career, Mode, Outcome};
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
        let mut w = World::for_career(Rng::Constant(0.5), (1280.0, 720.0), &c);
        w.banked = input["banked"].as_f64().unwrap();
        w.grudge = input["grudge"].as_f64().unwrap();
        w.stats.jumpers = input["jumpers"].as_u64().unwrap() as u32;
        w.rep_delta.rivals = input["rivalsDelta"].as_f64().unwrap();
        let st = c.settle(&w);
        let title = s["title"].as_str().unwrap();
        let ctx = format!("{input} -> {title}");
        check_career(&c, &s["career"], &ctx);
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
    c.extra.insert("belt".into(), Value::from("kessler"));
    c.extra.insert("crew".into(), Value::from("broker"));
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
    // Unique and not-yet-ported parts can't be bought.
    assert!(c.buy_part(Part::Grapple).is_err());
    assert!(c.buy_part(Part::Missiles).is_err());
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
