//! Keyboard and gamepad, merged into one [`Input`] like readInput and
//! pollGamepad in src/js/12-input.js, with the key bindings from Settings.

use crate::settings::{Bind, Coop, Settings, KEY_ACTIONS};
use cj_sim::coop::P2Input;
use cj_sim::{Action, Input};
use gilrs::ff::{BaseEffect, BaseEffectType, Effect, EffectBuilder, Repeat, Replay, Ticks};
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
    /// Toggle Marge's voice (V).
    pub voice: bool,
    /// Skip the guided training (T).
    pub skip_training: bool,
    /// Toggle the music (N).
    pub music: bool,
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
    /// The same for menu buttons, tracked separately so a press is seen by
    /// both readers whichever screen is up.
    menu_prev: Vec<Button>,
    pub last_device: Device,
    /// Keys held for each rebindable action, in KEY_ACTIONS order.
    binds: Vec<Vec<KeyCode>>,
    /// Who flies the co-op partner (with "pad", player 1 ignores that pad).
    coop: Coop,
    /// The rumble playing now; gilrs stops an effect when its handle drops.
    rumbling: Option<Effect>,
}

/// Button → one-shot, as PAD_TAPS: Y hook, B drop last, LB cut all,
/// X grind, D-pad up PvP on/off, Start pause, Back mute.
const PAD_TAPS: [(Button, Tap); 7] = [
    (Button::North, Tap::Act(Action::Hook)),
    (Button::East, Tap::Act(Action::Drop)),
    (Button::LeftTrigger, Tap::Act(Action::Cut)),
    (Button::West, Tap::Act(Action::Grind)),
    // D-pad up: down is the brake, and left and right turn.
    (Button::DPadUp, Tap::Act(Action::TogglePvp)),
    (Button::Start, Tap::Pause),
    (Button::Select, Tap::Mute),
];

/// Pad buttons the menus read: A press, LB / RB tabs, Start primary.
const MENU_BUTTONS: [Button; 4] = [
    Button::South,
    Button::LeftTrigger,
    Button::RightTrigger,
    Button::Start,
];

/// One frame of menu controls, before key-repeat.
#[derive(Clone, Copy, Debug, Default)]
pub struct MenuKeys {
    /// Direction held (arrow keys, D-pad or left stick).
    pub dir: Option<(f32, f32)>,
    pub activate: bool,
    pub tab: i32,
    pub start: bool,
}

impl Controls {
    pub fn new() -> Controls {
        Controls {
            // A machine without gamepad support still plays on the keyboard.
            gilrs: Gilrs::new().ok(),
            prev: Vec::new(),
            menu_prev: Vec::new(),
            last_device: Device::Keyboard,
            binds: Vec::new(),
            coop: Coop::Off,
            rumbling: None,
        }
        .bound(&Settings::default())
    }

    fn bound(mut self, s: &Settings) -> Controls {
        self.bind(s);
        self
    }

    /// Take the key bindings from the settings (rebuildKeyIndex).
    pub fn bind(&mut self, s: &Settings) {
        self.binds = KEY_ACTIONS.iter().map(|a| s.keys_for(a.0)).collect();
        self.coop = s.coop;
    }

    /// readP2: P2's controls from the numpad, or from the last connected
    /// gamepad (stick turn, RT or A thrust, LT brake, RB fire, Y latch).
    pub fn p2(&mut self) -> P2Input {
        let on = |k: KeyCode| if is_key_down(k) { 1.0 } else { 0.0 };
        match self.coop {
            Coop::Off => P2Input::default(),
            Coop::Numpad => P2Input {
                thrust: on(KeyCode::Kp8),
                brake: on(KeyCode::Kp5).max(on(KeyCode::Kp2)),
                turn: on(KeyCode::Kp6) - on(KeyCode::Kp4),
                fire: is_key_down(KeyCode::Kp0),
                latch: is_key_down(KeyCode::KpEnter),
            },
            Coop::Pad => {
                let Some(g) = self.gilrs.as_ref() else {
                    return P2Input::default();
                };
                let Some(id) = p2_pad(g) else {
                    return P2Input::default();
                };
                let pad = g.gamepad(id);
                let val = |b: Button| f64::from(pad.button_data(b).map_or(0.0, |d| d.value()));
                P2Input {
                    thrust: val(Button::RightTrigger2).max(if pad.is_pressed(Button::South) {
                        1.0
                    } else {
                        0.0
                    }),
                    brake: val(Button::LeftTrigger2),
                    turn: f64::from(deadzone(pad.value(Axis::LeftStickX))),
                    fire: pad.is_pressed(Button::RightTrigger),
                    latch: pad.is_pressed(Button::North),
                }
            }
        }
    }

    /// The pad player 1 may use: with gamepad co-op, never P2's.
    fn exclude(&self) -> Option<gilrs::GamepadId> {
        let g = self.gilrs.as_ref()?;
        (self.coop == Coop::Pad).then(|| p2_pad(g)).flatten()
    }

    fn keys(&self, b: Bind) -> &[KeyCode] {
        let i = KEY_ACTIONS.iter().position(|a| a.0 == b).expect("listed");
        &self.binds[i]
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
        let held = |b: Bind| self.keys(b).iter().any(|k| is_key_down(*k));
        let on = |b: bool| if b { 1.0f32 } else { 0.0 };
        let kb_turn = on(held(Bind::Right)) - on(held(Bind::Left));
        let kb_strafe = on(held(Bind::StrafeR)) - on(held(Bind::StrafeL));
        let kb_thrust = on(held(Bind::Thrust));
        let kb_reverse = on(held(Bind::Brake));
        let kb_fire = held(Bind::Fire);
        let kb_burn = [KeyCode::LeftShift, KeyCode::RightShift]
            .iter()
            .any(|k| is_key_down(*k));
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
        let tapped = |b: Bind| self.keys(b).iter().any(|k| is_key_pressed(*k));
        for (b, a) in [
            (Bind::Hook, Action::Hook),
            (Bind::Drop, Action::Drop),
            (Bind::Cut, Action::Cut),
            (Bind::Grind, Action::Grind),
            (Bind::Pvp, Action::TogglePvp),
        ] {
            if tapped(b) {
                out.actions.push(a);
            }
        }
        out.pause = key(KeyCode::P) || key(KeyCode::Escape);
        out.mute = key(KeyCode::M);
        out.voice = key(KeyCode::V);
        out.skip_training = key(KeyCode::T);
        out.music = key(KeyCode::N);
        if !out.actions.is_empty() || out.pause || out.mute {
            self.last_device = Device::Keyboard;
        }

        let skip = self.exclude();
        let Some(g) = self.gilrs.as_mut() else {
            return out;
        };
        let now: Vec<Button> = match active_pad(g, skip) {
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

    /// Menu controls this frame (menuNav): arrows, D-pad or the left stick
    /// point, Enter / Space / A press, LB / RB step tabs, Start presses
    /// the screen's primary button.
    pub fn menu(&mut self) -> MenuKeys {
        let mut out = MenuKeys::default();
        let held = |k: KeyCode| is_key_down(k);
        let mut dir = if held(KeyCode::Up) {
            Some((0.0, -1.0))
        } else if held(KeyCode::Down) {
            Some((0.0, 1.0))
        } else if held(KeyCode::Left) {
            Some((-1.0, 0.0))
        } else if held(KeyCode::Right) {
            Some((1.0, 0.0))
        } else {
            None
        };
        out.activate = is_key_pressed(KeyCode::Enter)
            || is_key_pressed(KeyCode::KpEnter)
            || is_key_pressed(KeyCode::Space);
        if dir.is_some() || out.activate {
            self.last_device = Device::Keyboard;
        }
        let skip = self.exclude();
        let Some(g) = self.gilrs.as_mut() else {
            out.dir = dir;
            return out;
        };
        let mut now = Vec::new();
        if let Some(id) = active_pad(g, skip) {
            let pad = g.gamepad(id);
            let p = |b: Button| pad.is_pressed(b);
            let (ax, ay) = (pad.value(Axis::LeftStickX), pad.value(Axis::LeftStickY));
            // gilrs reports stick up as positive Y; the browser's axis 1 is down.
            let pad_dir = if p(Button::DPadUp) || ay > 0.6 {
                Some((0.0, -1.0))
            } else if p(Button::DPadDown) || ay < -0.6 {
                Some((0.0, 1.0))
            } else if p(Button::DPadLeft) || ax < -0.6 {
                Some((-1.0, 0.0))
            } else if p(Button::DPadRight) || ax > 0.6 {
                Some((1.0, 0.0))
            } else {
                None
            };
            if pad_dir.is_some() {
                dir = pad_dir;
                self.last_device = Device::Pad;
            }
            now = MENU_BUTTONS.iter().copied().filter(|b| p(*b)).collect();
        }
        let edge = |b: Button| now.contains(&b) && !self.menu_prev.contains(&b);
        if edge(Button::South) {
            out.activate = true;
        }
        if edge(Button::LeftTrigger) {
            out.tab = -1;
        }
        if edge(Button::RightTrigger) {
            out.tab = 1;
        }
        out.start = edge(Button::Start);
        if now.iter().any(|b| edge(*b)) {
            self.last_device = Device::Pad;
        }
        self.menu_prev = now;
        out.dir = dir;
        out
    }

    /// rumble(): a short buzz, `strength` 0..1 for `ms` milliseconds, on
    /// the pad in use. Silently ignored on pads without motors.
    pub fn rumble(&mut self, strength: f64, ms: f64) {
        if self.last_device != Device::Pad {
            return;
        }
        let skip = self.exclude();
        let Some(g) = self.gilrs.as_mut() else { return };
        let Some(id) = active_pad(g, skip).filter(|id| g.gamepad(*id).is_ff_supported()) else {
            return;
        };
        let play_for = Ticks::from_ms(ms as u32);
        let motor = |kind| BaseEffect {
            kind,
            scheduling: Replay {
                play_for,
                ..Default::default()
            },
            envelope: Default::default(),
        };
        let magnitude = |level: f64| (level.clamp(0.0, 1.0) * f64::from(u16::MAX)) as u16;
        // The browser's dual-rumble: the weak motor at 60% of the strong one.
        let effect = EffectBuilder::new()
            .add_effect(motor(BaseEffectType::Strong {
                magnitude: magnitude(strength),
            }))
            .add_effect(motor(BaseEffectType::Weak {
                magnitude: magnitude(strength * 0.6),
            }))
            .repeat(Repeat::For(play_for))
            .gamepads(&[id])
            .finish(g);
        if let Ok(effect) = effect {
            if effect.play().is_ok() {
                self.rumbling = Some(effect);
            }
        }
    }

    fn pad_state(&mut self) -> Option<PadState> {
        let skip = self.exclude();
        let g = self.gilrs.as_mut()?;
        let id = active_pad(g, skip)?;
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

/// The last connected gamepad (P2's in gamepad co-op).
fn p2_pad(g: &Gilrs) -> Option<gilrs::GamepadId> {
    g.gamepads()
        .filter(|(_, p)| p.is_connected())
        .map(|(id, _)| id)
        .max_by_key(|id| usize::from(*id))
}

/// The connected pad used most recently (the browser sorts by timestamp),
/// leaving out `exclude`.
fn active_pad(g: &Gilrs, exclude: Option<gilrs::GamepadId>) -> Option<gilrs::GamepadId> {
    g.gamepads()
        .filter(|(id, p)| p.is_connected() && Some(*id) != exclude)
        .max_by_key(|(_, p)| {
            p.state()
                .buttons()
                .map(|(_, d)| d.timestamp())
                .chain(p.state().axes().map(|(_, d)| d.timestamp()))
                .max()
        })
        .map(|(id, _)| id)
}
