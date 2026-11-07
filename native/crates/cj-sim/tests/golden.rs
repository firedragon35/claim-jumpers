//! Golden traces: the Rust simulation must reproduce the browser's
//! recorded state, step by step, for each scenario in golden/scenarios.json.
//! Regenerate the traces with native/tools/golden/export.mjs.

use cj_sim::consts::{rock_size, STEP};
use cj_sim::entities::{Body, Rock, Wreck};
use cj_sim::items::ItemType;
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
    let s = &sc["ship"];
    let ship = &mut w.ship;
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
        w.wrecks.push(Wreck {
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
            hp: 33.0,
            max_hp: 33.0,
            contents: Vec::new(),
            crabs: 1,
            pts: Vec::new(),
            flash: 0.0,
            light: PI,
            dead: false,
        });
    }
    w.cam = Camera {
        x: w.ship.b.x,
        y: w.ship.b.y,
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
        ..Input::default()
    }
}

/// Compare one recorded row of numbers with ours; returns the first mismatch.
fn cmp_row(what: &str, want: &Value, got: &[f64]) -> Result<(), String> {
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

fn check_step(w: &World, want: &Value) -> Result<(), String> {
    let s = &w.ship;
    cmp_row(
        "ship",
        &want["s"],
        &[s.b.x, s.b.y, s.b.vx, s.b.vy, s.angle, s.av, s.hp, s.shield],
    )?;
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
                    if it.towed { 1.0 } else { 0.0 },
                    it.angle,
                ]
            })
            .collect(),
    )?;
    let chain: Vec<f64> = w
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
            .map(|x| vec![x.b.x, x.b.y, x.b.vx, x.b.vy, x.angle])
            .collect(),
    )?;
    cmp_row(
        "banked",
        &Value::from(vec![want["banked"].clone()]),
        &[w.banked],
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
    let trace = read_json(&format!("{name}.trace.json"));
    let mut w = build(sc, &trace["init"]);

    // The factories must agree with the browser's before any step runs.
    for (it, want) in w
        .items
        .iter()
        .zip(trace["init"]["items"].as_array().unwrap())
    {
        assert_eq!(
            it.label,
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

    let actions: Vec<(u64, Action)> = sc["actions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| {
            let act = match a["do"].as_str().unwrap() {
                "hook" => Action::Hook,
                "drop" => Action::Drop,
                "cut" => Action::Cut,
                other => panic!("unknown action {other}"),
            };
            (a["at"].as_u64().unwrap(), act)
        })
        .collect();
    let steps = trace["steps"].as_array().unwrap();
    let mut k = 0u64;
    for seg in sc["inputs"].as_array().unwrap() {
        let input = input_for(seg["keys"].as_array().unwrap());
        for _ in 0..seg["n"].as_u64().unwrap() {
            for (_, a) in actions.iter().filter(|(at, _)| *at == k) {
                w.action(*a);
            }
            w.step(&input, STEP);
            if let Err(e) = check_step(&w, &steps[k as usize]) {
                panic!("{name}, step {k}: {e}");
            }
            k += 1;
        }
    }
    assert_eq!(k as usize, steps.len(), "{name}: step count");
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
