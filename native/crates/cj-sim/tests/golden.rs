//! Golden traces: the simulation must reproduce its recorded state, step by
//! step, for each scenario in golden/scenarios.json. They began as recordings
//! of the browser game (1.0.7). This code now leads, so after a deliberate
//! change, re-record with `UPDATE_GOLDEN=1 cargo test -p cj-sim --test golden`
//! and review which traces moved.

use cj_sim::consts::{rock_size, Variant, STEP};
use cj_sim::entities::{make_crab, Body, CrabState, JumperState, Rock, Wreck};
use cj_sim::items::ItemType;
use cj_sim::mods::compute_mods;
use cj_sim::parts::{Levels, Mounted, Part, SLOTS, SYSTEMS};
use cj_sim::rng::Rng;
use cj_sim::world::Camera;
use cj_sim::{Action, Input, World};
use serde_json::Value;
use std::f64::consts::PI;
use std::path::PathBuf;

/// Relative tolerance. JavaScript and Rust share IEEE f64 arithmetic, but
/// sin, cos and exp come from different maths libraries and can differ in
/// the last bit; this allows that drift while still catching any rule or
/// ordering difference, which shows up orders of magnitude larger.
const TOL: f64 = 1e-7;

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

thread_local! {
    /// The step being rewritten, while re-recording.
    static RECORDING: std::cell::RefCell<Option<Value>> = const { std::cell::RefCell::new(None) };
}

/// While re-recording, store what was computed instead of comparing it.
fn record(what: &str, got: Value) -> bool {
    RECORDING.with_borrow_mut(|step| {
        let Some(step) = step else { return false };
        let key = if what == "ship" { "s" } else { what };
        let scalar = matches!(what, "grudge" | "banked");
        step[key] = if scalar { got[0].clone() } else { got };
        true
    })
}

fn read_json(name: &str) -> Value {
    let path = golden_dir().join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).expect("valid JSON")
}

fn num(v: &Value, key: &str, default: f64) -> f64 {
    v.get(key).and_then(Value::as_f64).unwrap_or(default)
}

/// Build the scenario's field exactly as the exporter does.
fn build(sc: &Value, init: &Value) -> World {
    let view = &init["view"];
    let mut w = World::empty(
        Rng::Constant(0.5),
        (view[0].as_f64().unwrap(), view[1].as_f64().unwrap()),
    );
    // An optional loadout, applied as the exporter does: the career's
    // levels (all 1 for a new career) overridden by the scenario's.
    if let Some(m) = sc.get("mounted") {
        let mut mounted: Mounted = [None; 5];
        for (i, s) in SLOTS.iter().enumerate() {
            mounted[i] = m[s.key].as_str().and_then(Part::from_key);
        }
        let mut levels = Levels::default();
        for s in SYSTEMS {
            if let Some(v) = sc["levels"][s.key()].as_u64() {
                levels.set(s, v as u8);
            }
        }
        w.pilots[0].mods = compute_mods(
            &mounted,
            &levels,
            sc["tugClass"].as_u64().unwrap_or(0) as usize,
        );
        w.pilots[0].ship.b.mass = w.pilots[0].mods.mass;
        w.pilots[0].ship.hp = w.pilots[0].mods.max_hp;
        w.pilots[0].ship.shield = w.pilots[0].mods.shield_max;
    }
    let s = &sc["ship"];
    let ship = &mut w.pilots[0].ship;
    ship.b.x = num(s, "x", ship.b.x);
    ship.b.y = num(s, "y", ship.b.y);
    ship.b.vx = num(s, "vx", 0.0);
    ship.b.vy = num(s, "vy", 0.0);
    ship.angle = num(s, "angle", ship.angle);
    ship.av = 0.0;
    ship.hp = num(s, "hp", ship.hp);
    ship.shield = num(s, "shield", ship.shield);
    ship.invuln = num(s, "invuln", ship.invuln);

    for it in sc["items"].as_array().unwrap() {
        let kind = ItemType::from_key(it["type"].as_str().unwrap()).unwrap();
        let item = w.new_item(
            kind,
            num(it, "x", 0.0),
            num(it, "y", 0.0),
            num(it, "vx", 0.0),
            num(it, "vy", 0.0),
        );
        w.items.push(item);
    }
    // Rocks and wrecks mirror the exporter's hand-built bodies (every roll 0.5).
    for r in sc["rocks"].as_array().unwrap() {
        let size = r["size"].as_u64().unwrap() as u8;
        let (rad, hp) = rock_size(size);
        w.rocks.push(Rock {
            id: cj_sim::entities::next_rock_id(),
            size,
            b: Body {
                x: num(r, "x", 0.0),
                y: num(r, "y", 0.0),
                vx: num(r, "vx", 0.0),
                vy: num(r, "vy", 0.0),
                r: rad,
                mass: rad * rad / 180.0,
                fixed: false,
            },
            hp: num(r, "hp", hp),
            veined: r["veined"].as_bool().unwrap_or(false),
            angle: PI,
            spin: 0.0,
            shape: Vec::new(),
            craters: Vec::new(),
            flash: 0.0,
            crumble_t: None,
            dead: false,
        });
    }
    for wr in sc["wrecks"].as_array().unwrap() {
        let contents: Vec<ItemType> = match wr["contents"].as_array() {
            Some(list) => list
                .iter()
                .map(|k| ItemType::from_key(k.as_str().unwrap()).unwrap())
                .collect(),
            None => vec![ItemType::Plate, ItemType::Plate, ItemType::Scrap],
        };
        let hp = 24.0 + contents.len() as f64 * 3.0;
        w.wrecks.push(Wreck {
            locked: false,
            freighter: false,
            b: Body {
                x: num(wr, "x", 0.0),
                y: num(wr, "y", 0.0),
                vx: num(wr, "vx", 0.0),
                vy: num(wr, "vy", 0.0),
                r: 102.5,
                mass: 60.0,
                fixed: false,
            },
            angle: num(wr, "angle", PI),
            spin: num(wr, "spin", 0.0),
            hp,
            max_hp: hp,
            contents,
            crabs: wr["crabs"].as_i64().unwrap_or(1) as i32,
            pts: Vec::new(),
            flash: 0.0,
            light: PI,
            claim: None,
            dead: false,
        });
    }
    for j in sc["jumpers"].as_array().into_iter().flatten() {
        let variant = match j["variant"].as_str().unwrap() {
            "skiff" => Variant::Skiff,
            "cutter" => Variant::Cutter,
            "brute" => Variant::Brute,
            "gunship" => Variant::Gunship,
            other => panic!("unknown variant {other}"),
        };
        let jm = w.new_jumper(num(j, "x", 0.0), num(j, "y", 0.0), Some(variant));
        w.jumpers.push(jm);
    }
    for c in sc["crabs"].as_array().into_iter().flatten() {
        let state = match c["state"].as_str().unwrap() {
            "ride" => CrabState::Ride,
            "crawl" => CrabState::Crawl,
            "latched" => CrabState::Latched,
            "free" => CrabState::Free,
            other => panic!("unknown crab state {other}"),
        };
        let host = c["host"].as_u64().map(|i| &w.items[i as usize]);
        let (hx, hy) = host.map_or((0.0, 0.0), |h| (h.b.x, h.b.y));
        let host_id = host.map(|h| h.id);
        let crab = make_crab(&mut w.rng, state, host_id, num(c, "x", hx), num(c, "y", hy));
        w.crabs.push(crab);
    }
    w.grudge = num(sc, "grudge", 0.0);
    // The exporter's shift has a new Standard career behind it.
    let mut career = cj_sim::career::Career::new(cj_sim::career::Mode::Standard);
    if let Some(v) = sc["wallet"].as_f64() {
        career.wallet = v;
    }
    w.pilots[0].career = Some(career);
    if let Some(v) = sc["timeLeft"].as_f64() {
        w.time_left = v;
    }
    for c in sc["collectors"].as_array().into_iter().flatten() {
        w.spawn_collector(0);
        let k = w.collectors.last_mut().unwrap();
        k.b.x = num(c, "x", k.b.x);
        k.b.y = num(c, "y", k.b.y);
    }
    for s in sc["sheriffs"].as_array().into_iter().flatten() {
        w.spawn_sheriff(0, s["responding"].as_bool().unwrap_or(false));
        let k = w.sheriffs.last_mut().unwrap();
        k.b.x = num(s, "x", k.b.x);
        k.b.y = num(s, "y", k.b.y);
    }
    if let Some(ms) = sc["miners"].as_array() {
        w.spawn_miners(ms.len() as i32);
        for (m, s) in w.miners.iter_mut().zip(ms) {
            m.b.x = num(s, "x", m.b.x);
            m.b.y = num(s, "y", m.b.y);
        }
    }
    if let Some(r) = sc.get("refinery") {
        w.spawn_refinery();
        let k = w.refinery.as_mut().unwrap();
        k.spin = num(r, "spin", k.spin);
        k.hunger = num(r, "hunger", k.hunger);
        k.jaw_hp = num(r, "jawHp", k.jaw_hp);
        if r["state"].as_str() == Some("hungry") {
            k.state = cj_sim::opposition::RefineryState::Hungry;
        }
    }
    let n = sc["sentries"].as_u64().unwrap_or(0) as usize;
    for i in 0..n {
        let s = w.make_sentry(0, i, n);
        w.sentries.push(s);
    }
    if let Some(n) = sc["wanted"].as_u64() {
        let (x, y) = (w.pilots[0].ship.b.x, w.pilots[0].ship.b.y);
        w.add_wanted(0, n as u32, x, y);
    }
    if let Some(v) = sc["reinforceT"].as_f64() {
        w.reinforce_t = Some(v);
    }
    w.cam = Camera {
        x: w.pilots[0].ship.b.x,
        y: w.pilots[0].ship.b.y,
        zoom: 1.0,
    };
    w
}

fn input_for(keys: &[Value]) -> Input {
    let held = |k: &str| keys.iter().any(|v| v.as_str() == Some(k));
    let on = |b: bool| if b { 1.0 } else { 0.0 };
    Input {
        thrust: on(held("thrust")),
        reverse: on(held("brake")),
        turn: on(held("right")) - on(held("left")),
        strafe: on(held("strafeR")) - on(held("strafeL")),
        fire: held("fire"),
        burn: held("burn"),
    }
}

/// Compare one recorded row of numbers with ours; returns the first mismatch.
fn cmp_row(what: &str, want: &Value, got: &[f64]) -> Result<(), String> {
    if record(what, Value::from(got.to_vec())) {
        return Ok(());
    }
    let want = want.as_array().unwrap();
    if want.len() != got.len() {
        return Err(format!(
            "{what}: {} fields recorded, {} computed",
            want.len(),
            got.len()
        ));
    }
    for (i, (w, g)) in want.iter().zip(got).enumerate() {
        let w = w.as_f64().unwrap();
        if (w - g).abs() > TOL * w.abs().max(1.0) {
            return Err(format!("{what}[{i}]: browser {w}, rust {g}"));
        }
    }
    Ok(())
}

fn cmp_list(what: &str, want: &Value, got: Vec<Vec<f64>>) -> Result<(), String> {
    let rows: Vec<Value> = got.iter().map(|r| Value::from(r.clone())).collect();
    if record(what, Value::from(rows)) {
        return Ok(());
    }
    let want = want.as_array().unwrap();
    if want.len() != got.len() {
        return Err(format!(
            "{what}: browser has {}, rust has {}",
            want.len(),
            got.len()
        ));
    }
    for (i, (w, g)) in want.iter().zip(&got).enumerate() {
        cmp_row(&format!("{what}[{i}]"), w, g)?;
    }
    Ok(())
}

/// Index of an item id in the items list, or -1, as the exporter records it.
fn item_pos(w: &World, id: Option<u32>) -> f64 {
    id.and_then(|id| w.item_index(id))
        .map_or(-1.0, |i| i as f64)
}

fn check_step(w: &World, want: &Value) -> Result<(), String> {
    let s = &w.pilots[0].ship;
    let on = |b: bool| if b { 1.0 } else { 0.0 };
    cmp_row(
        "ship",
        &want["s"],
        &[
            s.b.x,
            s.b.y,
            s.b.vx,
            s.b.vy,
            s.angle,
            s.av,
            s.hp,
            s.shield,
            s.heat,
            on(s.overheated),
            s.fire_t,
            s.spin_meter,
            on(s.alive),
            f64::from(s.slugs),
            s.stun_t,
            s.burn_t,
            s.engine_heat,
            s.engine_over_t,
            s.net.map_or(-1.0, |n| n.hp),
            f64::from(s.missiles),
        ],
    )?;
    use cj_sim::opposition::{CollectorState, RefineryState, SheriffState};
    cmp_list(
        "drones",
        &want["drones"],
        w.drones
            .iter()
            .map(|d| vec![d.b.x, d.b.y, d.b.vx, d.b.vy, d.hp])
            .collect(),
    )?;
    cmp_list(
        "collectors",
        &want["collectors"],
        w.collectors
            .iter()
            .map(|c| {
                let st = match c.state {
                    CollectorState::Approach => 0.0,
                    CollectorState::Tow => 1.0,
                    CollectorState::Recover => 2.0,
                    CollectorState::Leave => 3.0,
                };
                vec![c.b.x, c.b.y, c.b.vx, c.b.vy, c.hp, st, c.tether]
            })
            .collect(),
    )?;
    cmp_list(
        "nets",
        &want["nets"],
        w.nets.iter().map(|n| vec![n.x, n.y, n.life]).collect(),
    )?;
    cmp_list(
        "sheriffs",
        &want["sheriffs"],
        w.sheriffs
            .iter()
            .map(|s| {
                let st = match s.state {
                    SheriffState::Patrol => 0.0,
                    SheriffState::Pursuit => 1.0,
                    SheriffState::Leave => 2.0,
                };
                vec![s.b.x, s.b.y, s.b.vx, s.b.vy, s.hp, st, s.cuff]
            })
            .collect(),
    )?;
    cmp_list(
        "miners",
        &want["miners"],
        w.miners
            .iter()
            .map(|m| vec![m.b.x, m.b.y, m.b.vx, m.b.vy, m.hp, m.angry_t, m.drill])
            .collect(),
    )?;
    cmp_list(
        "missiles",
        &want["missiles"],
        w.missiles
            .iter()
            .map(|m| vec![m.x, m.y, m.vx, m.vy, m.life])
            .collect(),
    )?;
    cmp_list(
        "sentries",
        &want["sentries"],
        w.sentries
            .iter()
            .map(|d| vec![d.x, d.y, d.hp, on(d.rogue)])
            .collect(),
    )?;
    let refinery: Vec<f64> = w.refinery.as_ref().map_or(Vec::new(), |r| {
        let st = match r.state {
            RefineryState::Dormant => 0.0,
            RefineryState::Locked => 1.0,
            RefineryState::Armed => 2.0,
            RefineryState::Open => 3.0,
            RefineryState::Hungry => 4.0,
            RefineryState::Wrecked => 5.0,
        };
        let filled = r.sockets.iter().filter(|k| k.filled).count() as f64;
        vec![
            r.b.x, r.b.y, r.angle, st, r.hunger, r.pull_r, r.jaw_hp, filled,
        ]
    });
    cmp_row("refinery", &want["refinery"], &refinery)?;
    cmp_row(
        "law",
        &want["law"],
        &[
            f64::from(w.pilots[0].wanted),
            w.pilots[0].fines,
            f64::from(w.pilots[0].arrests),
        ],
    )?;
    let types: Vec<&str> = w.items.iter().map(|it| it.kind.key()).collect();
    let want_types: Vec<&str> = want["types"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    if !record("types", Value::from(types.clone())) && types != want_types {
        return Err(format!(
            "item types: browser {want_types:?}, rust {types:?}"
        ));
    }
    cmp_list(
        "items",
        &want["items"],
        w.items
            .iter()
            .map(|it| {
                vec![
                    it.b.x,
                    it.b.y,
                    it.b.vx,
                    it.b.vy,
                    it.link_len,
                    it.tension,
                    on(it.towed),
                    it.angle,
                    it.hp.unwrap_or(-1.0),
                    it.fuse,
                    it.hooked_by
                        .and_then(|id| w.jumper_index(id))
                        .map_or(-1.0, |j| j as f64),
                ]
            })
            .collect(),
    )?;
    let chain: Vec<f64> = w.pilots[0]
        .chain
        .iter()
        .map(|id| w.item_index(*id).unwrap() as f64)
        .collect();
    cmp_row("chain", &want["chain"], &chain)?;
    cmp_list(
        "rocks",
        &want["rocks"],
        w.rocks
            .iter()
            .map(|r| {
                vec![
                    r.b.x,
                    r.b.y,
                    r.b.vx,
                    r.b.vy,
                    r.hp,
                    f64::from(r.size),
                    r.angle,
                ]
            })
            .collect(),
    )?;
    cmp_list(
        "wrecks",
        &want["wrecks"],
        w.wrecks
            .iter()
            .map(|x| {
                vec![
                    x.b.x,
                    x.b.y,
                    x.b.vx,
                    x.b.vy,
                    x.angle,
                    x.hp,
                    x.contents.len() as f64,
                    f64::from(x.crabs),
                ]
            })
            .collect(),
    )?;
    cmp_list(
        "jumpers",
        &want["jumpers"],
        w.jumpers
            .iter()
            .map(|j| {
                vec![
                    j.b.x,
                    j.b.y,
                    j.b.vx,
                    j.b.vy,
                    j.hp,
                    on(j.state == JumperState::Haul),
                    j.link_len,
                    j.latch_t,
                    j.fire_t,
                    j.angle,
                    item_pos(w, j.hook),
                    j.flee_t,
                    j.cover_t,
                ]
            })
            .collect(),
    )?;
    cmp_list(
        "crabs",
        &want["crabs"],
        w.crabs
            .iter()
            .map(|c| {
                let state = match c.state {
                    CrabState::Ride => 0.0,
                    CrabState::Crawl => 1.0,
                    CrabState::Latched => 2.0,
                    CrabState::Free => 3.0,
                };
                vec![c.x, c.y, state, c.t, item_pos(w, c.host)]
            })
            .collect(),
    )?;
    let shots = |v: &[cj_sim::entities::Bullet]| {
        v.iter()
            .map(|b| vec![b.x, b.y, b.vx, b.vy, b.life])
            .collect::<Vec<_>>()
    };
    cmp_list("bullets", &want["bullets"], shots(&w.bullets))?;
    cmp_list("ebullets", &want["ebullets"], shots(&w.ebullets))?;
    cmp_row(
        "grudge",
        &Value::from(vec![want["grudge"].clone()]),
        &[w.grudge],
    )?;
    cmp_row(
        "banked",
        &Value::from(vec![want["banked"].clone()]),
        &[w.pilots[0].banked],
    )?;
    cmp_row("cam", &want["cam"], &[w.cam.x, w.cam.y, w.cam.zoom])
}

fn run(name: &str) {
    let spec = read_json("scenarios.json");
    let sc = spec["scenarios"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == name)
        .expect("scenario exists");
    let mut trace = read_json(&format!("{name}.trace.json"));
    let update = std::env::var_os("UPDATE_GOLDEN").is_some();
    let mut w = build(sc, &trace["init"]);

    // The factories must agree with the browser's before any step runs.
    for (it, want) in w
        .items
        .iter()
        .zip(trace["init"]["items"].as_array().unwrap())
    {
        assert_eq!(
            it.label(),
            want["label"].as_str().unwrap(),
            "{name}: item grade"
        );
        assert_eq!(
            it.b.mass,
            want["mass"].as_f64().unwrap(),
            "{name}: item mass"
        );
        assert_eq!(
            it.value,
            want["value"].as_f64().unwrap(),
            "{name}: item value"
        );
    }

    let actions: Vec<(u64, Option<Action>)> = sc["actions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| {
            let act = match a["do"].as_str().unwrap() {
                // Wrecking the tug outright (killShip) is not a player action.
                "kill" => None,
                "hook" => Some(Action::Hook),
                "drop" => Some(Action::Drop),
                "cut" => Some(Action::Cut),
                "grind" => Some(Action::Grind),
                other => panic!("unknown action {other}"),
            };
            (a["at"].as_u64().unwrap(), act)
        })
        .collect();
    let mut steps = trace["steps"].as_array().unwrap().clone();
    let mut k = 0u64;
    let mut moved = false;
    for seg in sc["inputs"].as_array().unwrap() {
        let input = input_for(seg["keys"].as_array().unwrap());
        for _ in 0..seg["n"].as_u64().unwrap() {
            for (_, a) in actions.iter().filter(|(at, _)| *at == k) {
                match a {
                    Some(a) => w.action(*a),
                    None => w.kill_ship(0),
                }
            }
            w.step(&input, STEP);
            let result = check_step(&w, &steps[k as usize]);
            if update {
                moved |= result.is_err();
                RECORDING.set(Some(steps[k as usize].clone()));
                check_step(&w, &steps[k as usize]).expect("recording cannot fail");
                steps[k as usize] = RECORDING.take().expect("recording");
            } else if let Err(e) = result {
                panic!("{name}, step {k}: {e}");
            }
            k += 1;
        }
    }
    assert_eq!(k as usize, steps.len(), "{name}: step count");
    // Only a trace that really moved is rewritten, so the diff shows what changed.
    if moved {
        trace["steps"] = Value::from(steps);
        let path = golden_dir().join(format!("{name}.trace.json"));
        std::fs::write(path, trace.to_string()).expect("write trace");
    }
}

#[test]
fn flight() {
    run("flight");
}
#[test]
fn tow() {
    run("tow");
}
#[test]
fn train() {
    run("train");
}
#[test]
fn impact() {
    run("impact");
}
#[test]
fn bank() {
    run("bank");
}
#[test]
fn carrier() {
    run("carrier");
}
#[test]
fn wall() {
    run("wall");
}
#[test]
fn cannon() {
    run("cannon");
}
#[test]
fn wreck() {
    run("wreck");
}
#[test]
fn rival() {
    run("rival");
}
#[test]
fn smash() {
    run("smash");
}
#[test]
fn crabs() {
    run("crabs");
}
#[test]
fn reactor() {
    run("reactor");
}
#[test]
fn strafe() {
    run("strafe");
}
#[test]
fn grinder() {
    run("grinder");
}
#[test]
fn scoop() {
    run("scoop");
}
#[test]
fn loadout() {
    run("loadout");
}
#[test]
fn afterburn() {
    run("afterburn");
}
#[test]
fn collector() {
    run("collector");
}
#[test]
fn law() {
    run("law");
}
#[test]
fn miners() {
    run("miners");
}
#[test]
fn refinery() {
    run("refinery");
}
#[test]
fn jaws() {
    run("jaws");
}
#[test]
fn rogue() {
    run("rogue");
}
