//! Claim Jumpers, native Windows build. Milestone 1: the core slice —
//! fly the tug, hook and tow salvage, collide with the field, bank cargo
//! at the carrier's bay, against the shift clock.
//!
//! `--smoke` runs an unattended check: a scripted hook-and-tow, one
//! simulation step per frame, a screenshot to
//! %TEMP%\claim-jumpers-m1-smoke.png, and a report on stdout.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod controls;
mod draw;
mod fx;
mod hud;
mod look;

use cj_sim::consts::{BAY_X, BAY_Y, MAX_FRAME_DT, MAX_STEPS_PER_FRAME, STEP};
use cj_sim::items::ItemType;
use cj_sim::rng::Rng;
use cj_sim::{Action, Input, World};
use macroquad::prelude::*;

fn window_conf() -> Conf {
    Conf {
        window_title: "Claim Jumpers".to_owned(),
        window_width: 1280,
        window_height: 720,
        high_dpi: true,
        sample_count: 4,
        window_resizable: true,
        ..Default::default()
    }
}

/// Logical window size (macroquad already divides out the DPI scale).
fn view() -> (f32, f32) {
    (screen_width(), screen_height())
}

/// Frames the smoke test runs before taking its screenshot.
const SMOKE_FRAMES: u32 = 150;

#[macroquad::main(window_conf)]
async fn main() {
    let smoke = std::env::args().any(|a| a == "--smoke");
    // The smoke run is seeded so every run checks the same field.
    let seed_rng = || {
        if smoke {
            Rng::seeded(7)
        } else {
            Rng::from_time()
        }
    };

    let look = look::Look::new();
    let mut controls = controls::Controls::new();
    let mut fx_rng = seed_rng();
    let stars = draw::Stars::new(&mut fx_rng);
    let mut vignette = draw::Vignette::new();
    let mut fx = fx::Fx::new(seed_rng());
    let (vw, vh) = view();
    let mut world = World::new_shift(seed_rng(), (f64::from(vw), f64::from(vh)));
    let mut accumulator = 0.0f64;
    let mut paused = false;
    let mut frame: u32 = 0;
    let mut steps: u64 = 0;
    let start = get_time();

    loop {
        controls.poll();
        let (vw, vh) = view();
        world.view = (f64::from(vw), f64::from(vh));
        let presses = controls.presses();

        if world.shift_over {
            // Placeholder until the end-of-shift report and career screens
            // (milestone 3): Enter starts a fresh shift.
            if is_key_pressed(KeyCode::Enter) {
                world = World::new_shift(Rng::from_time(), world.view);
                fx = fx::Fx::new(Rng::from_time());
                accumulator = 0.0;
            }
        } else if presses.pause {
            // Placeholder pause: freezes the shift. The pause menu
            // arrives with the other menus in milestone 3.
            paused = !paused;
        }

        if !paused && !world.shift_over {
            // One-shot cable commands land before the next step, as the
            // browser's key handlers run between frames.
            for a in &presses.actions {
                world.action(*a);
            }
            if smoke {
                smoke_script(&mut world, frame);
            }
            let input = if smoke {
                smoke_input(frame)
            } else {
                controls.sample()
            };
            // Fixed-step simulation; a long stall drops the backlog instead
            // of spiralling (frame() in src/js/19-boot.js).
            let n_max = if smoke { 1 } else { MAX_STEPS_PER_FRAME };
            accumulator += if smoke {
                STEP
            } else {
                f64::from(get_frame_time()).min(MAX_FRAME_DT)
            };
            let mut n = 0;
            while accumulator >= STEP && n < n_max && !world.shift_over {
                world.step(&input, STEP);
                fx.absorb(&world.events);
                world.events.clear();
                fx.emit_ambient(&world);
                fx.update(STEP);
                accumulator -= STEP;
                n += 1;
                steps += 1;
            }
            if n == MAX_STEPS_PER_FRAME {
                accumulator = 0.0;
            }
        }

        draw::render(draw::Scene {
            look: &look,
            world: &world,
            fx: &fx,
            stars: &stars,
            vignette: &mut vignette,
            rng: &mut fx_rng,
            device: controls.last_device,
            view: (vw, vh),
            dpi: screen_dpi_scale(),
        });
        hud::draw(&look, &world, (vw, vh), controls.last_device);
        if world.shift_over {
            let sub = format!(
                "Banked {} cr. Press Enter for a new shift.",
                cj_sim::world::fmt_credits(world.banked)
            );
            hud::banner(&look, (vw, vh), "SHIFT OVER", &sub);
        } else if paused {
            hud::banner(&look, (vw, vh), "PAUSED", "Press P or Esc to resume.");
        }

        if smoke && frame == SMOKE_FRAMES {
            smoke_report(&world, &fx, frame, steps, get_time() - start);
            std::process::exit(0);
        }
        frame += 1;
        next_frame().await;
    }
}

/// Smoke script: put a piece of ore beside the tug, then hook it.
fn smoke_script(w: &mut World, frame: u32) {
    if frame == 0 {
        // Start well clear of the bay, or the bay banks the ore at once.
        w.ship.b.x = BAY_X - 500.0;
        let it = w.new_item(ItemType::Ore, BAY_X - 500.0, BAY_Y + 110.0, 0.0, 0.0);
        w.items.push(it);
        w.ship.invuln = 0.0;
    }
    if frame == 2 {
        w.action(Action::Hook);
    }
}

/// Smoke flight: thrust away from the carrier, then swing the cargo round.
fn smoke_input(frame: u32) -> Input {
    Input {
        thrust: if frame < 120 { 1.0 } else { 0.0 },
        turn: if (40..100).contains(&frame) { 1.0 } else { 0.0 },
        ..Input::default()
    }
}

fn smoke_report(w: &World, fx: &fx::Fx, frame: u32, steps: u64, secs: f64) {
    let shot = std::env::temp_dir().join("claim-jumpers-m1-smoke.png");
    // The framebuffer reads bottom row first and carries the alpha of the
    // last blend; flip it and make it opaque so the PNG shows the window.
    let mut img = get_screen_data();
    let row = usize::from(img.width) * 4;
    let flipped: Vec<u8> = img.bytes.chunks(row).rev().flatten().copied().collect();
    img.bytes = flipped;
    for px in img.bytes.chunks_mut(4) {
        px[3] = 255;
    }
    img.export_png(&shot.to_string_lossy());
    let s = &w.ship;
    let finite = s.b.x.is_finite()
        && s.b.y.is_finite()
        && w.items
            .iter()
            .all(|i| i.b.x.is_finite() && i.b.y.is_finite());
    println!("M1 SMOKE REPORT");
    println!(
        "  frames: {frame} in {secs:.2} s ({:.0} fps), sim steps: {steps}",
        f64::from(frame) / secs.max(1e-9)
    );
    println!(
        "  view: {:.0} x {:.0} logical px, dpi scale {:.2}",
        w.view.0,
        w.view.1,
        screen_dpi_scale()
    );
    println!(
        "  field: {} rocks, {} wrecks, {} items",
        w.rocks.len(),
        w.wrecks.len(),
        w.items.len()
    );
    println!(
        "  tug: ({:.1}, {:.1}) speed {:.1}, hull {:.1}, shield {:.1}",
        s.b.x,
        s.b.y,
        s.b.speed(),
        s.hp,
        s.shield
    );
    println!(
        "  chain: {} item(s), {:.1} t; banked {} cr",
        w.chain.len(),
        w.tow_mass(),
        w.banked
    );
    println!(
        "  camera zoom {:.3}; particles {}",
        w.cam.zoom,
        fx.particles.len()
    );
    println!("  positions finite: {finite}");
    println!("  screenshot: {}", shot.display());
}
