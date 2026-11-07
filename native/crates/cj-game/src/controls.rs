//! Keyboard and gamepad, merged into one [`Input`] like readInput and
//! pollGamepad in src/js/12-input.js. Default key bindings only; rebinding
//! comes with the settings screen in milestone 5.

use cj_sim::{Action, Input};
use gilrs::{Axis, Button, Gilrs};
use macroquad::prelude::*;

/// Stick deadzone; the edge maps to 0 instead of jumping.
const PAD_DEADZONE: f32 = 0.22;

fn deadzone(v: f32) -> f32 {
    let a = v.abs();
    if a < PAD_DEADZONE {
        0.0
    } else {
        v.signum() * (a - PAD_DEADZONE) / (1.0 - PAD_DEADZONE)
    }
}

/// Which device last gave meaningful input; drives on-screen prompt labels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Device {
    Keyboard,
    Pad,
}

/// One-shot presses gathered this frame, beyond the cable actions.
#[derive(Default)]
pub struct Presses {
    pub actions: Vec<Action>,
    pub pause: bool,
    /// Toggle sound (M, or the pad's Back button).
    pub mute: bool,
}

/// What a pad button tap does.
#[derive(Clone, Copy)]
enum Tap {
    Act(Action),
    Pause,
    Mute,
}

pub struct Controls {
    gilrs: Option<Gilrs>,
    /// Buttons held last frame, for rising edges (holding Y must not hook every frame).
    prev: Vec<Button>,
    pub last_device: Device,
}

/// Button → one-shot, as PAD_TAPS: Y hook, B drop last, LB cut all,
/// Start pause, Back mute. (X grinds once the grinder arrives in milestone 3.)
const PAD_TAPS: [(Button, Tap); 5] = [
    (Button::North, Tap::Act(Action::Hook)),
    (Button::East, Tap::Act(Action::Drop)),
    (Button::LeftTrigger, Tap::Act(Action::Cut)),
    (Button::Start, Tap::Pause),
    (Button::Select, Tap::Mute),
];

impl Controls {
    pub fn new() -> Controls {
        Controls {
            // A machine without gamepad support still plays on the keyboard.
            gilrs: Gilrs::new().ok(),
            prev: Vec::new(),
            last_device: Device::Keyboard,
        }
    }

    /// Drain gamepad events once per frame, before reading anything, so
    /// gilrs updates its cached state and notices pads connected after launch.
    pub fn poll(&mut self) {
        if let Some(g) = self.gilrs.as_mut() {
            while g.next_event().is_some() {}
        }
    }

    /// Held controls right now, from keyboard and the most recently used pad.
    pub fn sample(&mut self) -> Input {
        let held = |keys: &[KeyCode]| keys.iter().any(|k| is_key_down(*k));
        let on = |b: bool| if b { 1.0f32 } else { 0.0 };
        let kb_turn =
            on(held(&[KeyCode::D, KeyCode::Right])) - on(held(&[KeyCode::A, KeyCode::Left]));
        let kb_strafe = on(held(&[KeyCode::C])) - on(held(&[KeyCode::Z]));
        let kb_thrust = on(held(&[KeyCode::W, KeyCode::Up]));
        let kb_reverse = on(held(&[KeyCode::S, KeyCode::Down]));
        let kb_fire = held(&[KeyCode::Space, KeyCode::J]);
        let kb_burn = held(&[KeyCode::LeftShift, KeyCode::RightShift]);
        if kb_turn != 0.0 || kb_thrust > 0.0 || kb_reverse > 0.0 || kb_fire || kb_strafe != 0.0 {
            self.last_device = Device::Keyboard;
        }

        let pad = self.pad_state();
        if let Some(p) = &pad {
            if p.turn != 0.0
                || p.strafe != 0.0
                || p.thrust > 0.1
                || p.reverse > 0.1
                || p.fire
                || p.any_button
            {
                self.last_device = Device::Pad;
            }
        }
        let p = pad.unwrap_or_default();
        Input {
            thrust: f64::from(kb_thrust.max(p.thrust)),
            reverse: f64::from(kb_reverse.max(p.reverse)),
            turn: f64::from((kb_turn + p.turn).clamp(-1.0, 1.0)),
            strafe: f64::from((kb_strafe + p.strafe).clamp(-1.0, 1.0)),
            fire: kb_fire || p.fire,
            burn: kb_burn || p.burn,
        }
    }

    /// One-shot presses since last frame: keys from macroquad, buttons by edge.
    pub fn presses(&mut self) -> Presses {
        let mut out = Presses::default();
        let key = |k: KeyCode| is_key_pressed(k);
        if key(KeyCode::E) || key(KeyCode::K) {
            out.actions.push(Action::Hook);
        }
        if key(KeyCode::Q) {
            out.actions.push(Action::Drop);
        }
        if key(KeyCode::X) {
            out.actions.push(Action::Cut);
        }
        out.pause = key(KeyCode::P) || key(KeyCode::Escape);
        out.mute = key(KeyCode::M);
        if !out.actions.is_empty() || out.pause || out.mute {
            self.last_device = Device::Keyboard;
        }

        let Some(g) = self.gilrs.as_mut() else {
            return out;
        };
        let now: Vec<Button> = match active_pad(g) {
            Some(id) => {
                let pad = g.gamepad(id);
                PAD_TAPS
                    .iter()
                    .map(|(b, _)| *b)
                    .filter(|b| pad.is_pressed(*b))
                    .collect()
            }
            None => Vec::new(),
        };
        for (b, act) in PAD_TAPS {
            if now.contains(&b) && !self.prev.contains(&b) {
                self.last_device = Device::Pad;
                match act {
                    Tap::Act(a) => out.actions.push(a),
                    Tap::Pause => out.pause = true,
                    Tap::Mute => out.mute = true,
                }
            }
        }
        self.prev = now;
        out
    }

    fn pad_state(&mut self) -> Option<PadState> {
        let g = self.gilrs.as_mut()?;
        let id = active_pad(g)?;
        let pad = g.gamepad(id);
        let val = |b: Button| pad.button_data(b).map(|d| d.value()).unwrap_or(0.0);
        let pressed = |b: Button| pad.is_pressed(b);
        let lt = val(Button::LeftTrigger2);
        let rt = val(Button::RightTrigger2);
        let dpad = (if pressed(Button::DPadRight) { 1.0 } else { 0.0 })
            - (if pressed(Button::DPadLeft) { 1.0 } else { 0.0 });
        let down = if pressed(Button::DPadDown) { 1.0 } else { 0.0 };
        // Both triggers squeezed together = afterburner (and not a brake).
        let burn = lt > 0.5 && rt > 0.5;
        Some(PadState {
            turn: (deadzone(pad.value(Axis::LeftStickX)) + dpad).clamp(-1.0, 1.0),
            thrust: rt.max(if pressed(Button::South) { 1.0 } else { 0.0 }),
            reverse: if burn { down } else { lt.max(down) },
            strafe: deadzone(pad.value(Axis::RightStickX)),
            fire: pressed(Button::RightTrigger),
            burn,
            any_button: [
                Button::South,
                Button::East,
                Button::North,
                Button::West,
                Button::LeftTrigger,
                Button::RightTrigger,
                Button::Start,
            ]
            .iter()
            .any(|b| pressed(*b)),
        })
    }
}

#[derive(Default)]
struct PadState {
    turn: f32,
    thrust: f32,
    reverse: f32,
    strafe: f32,
    fire: bool,
    burn: bool,
    any_button: bool,
}

/// The connected pad used most recently (the browser sorts by timestamp).
fn active_pad(g: &Gilrs) -> Option<gilrs::GamepadId> {
    g.gamepads()
        .filter(|(_, p)| p.is_connected())
        .max_by_key(|(_, p)| {
            p.state()
                .buttons()
                .map(|(_, d)| d.timestamp())
                .chain(p.state().axes().map(|(_, d)| d.timestamp()))
                .max()
        })
        .map(|(id, _)| id)
}
