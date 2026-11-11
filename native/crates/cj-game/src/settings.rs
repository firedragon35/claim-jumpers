//! Settings (src/js/05-settings.js): music, reduced motion, the
//! colorblind palette, local co-op and key bindings, stored as the browser
//! stores them (settings.json, key names as KeyboardEvent codes) and
//! validated on load. Marge's voice toggle is kept here too.

use macroquad::prelude::KeyCode;
use serde_json::{json, Map, Value};

/// Local co-op: who flies the partner tug.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Coop {
    Off,
    Numpad,
    Pad,
}

impl Coop {
    pub fn key(self) -> &'static str {
        match self {
            Coop::Off => "off",
            Coop::Numpad => "numpad",
            Coop::Pad => "pad",
        }
    }
    fn from_key(k: &str) -> Option<Coop> {
        [Coop::Off, Coop::Numpad, Coop::Pad]
            .into_iter()
            .find(|c| c.key() == k)
    }
    /// Off → numpad → gamepad → off, as the settings button cycles.
    pub fn next(self) -> Coop {
        match self {
            Coop::Off => Coop::Numpad,
            Coop::Numpad => Coop::Pad,
            Coop::Pad => Coop::Off,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Coop::Off => "off",
            Coop::Numpad => "numpad",
            Coop::Pad => "gamepad",
        }
    }
}

/// A rebindable action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bind {
    Thrust,
    Brake,
    Left,
    Right,
    Fire,
    StrafeL,
    StrafeR,
    Hook,
    Drop,
    Cut,
    Grind,
    /// Switch PvP on or off, in the bay of a server that allows it.
    Pvp,
}

/// [action, save id, label, default codes]: the first code is the one a
/// rebind replaces; any second is a fixed alternate (arrows, J, K).
pub const KEY_ACTIONS: [(Bind, &str, &str, &[&str]); 12] = [
    (Bind::Thrust, "thrust", "Thrust", &["KeyW", "ArrowUp"]),
    (Bind::Brake, "brake", "Brake", &["KeyS", "ArrowDown"]),
    (Bind::Left, "left", "Turn left", &["KeyA", "ArrowLeft"]),
    (Bind::Right, "right", "Turn right", &["KeyD", "ArrowRight"]),
    (Bind::Fire, "fire", "Fire", &["Space", "KeyJ"]),
    (Bind::StrafeL, "strafeL", "Strafe left", &["KeyZ"]),
    (Bind::StrafeR, "strafeR", "Strafe right", &["KeyC"]),
    (Bind::Hook, "hook", "Hook", &["KeyE", "KeyK"]),
    (Bind::Drop, "drop", "Drop last", &["KeyQ"]),
    (Bind::Cut, "cut", "Cut all", &["KeyX"]),
    (Bind::Grind, "grind", "Grind", &["KeyG"]),
    // B: V (Marge's voice) is reserved, and B is free in every default.
    (Bind::Pvp, "pvp", "PvP on/off", &["KeyB"]),
];

/// Fixed keys that can't be rebound (pause, mute, voice, music, skip
/// training, menus).
pub const RESERVED: [&str; 10] = [
    "KeyP",
    "Escape",
    "KeyM",
    "KeyV",
    "KeyN",
    "KeyT",
    "Enter",
    "Tab",
    "ShiftLeft",
    "ShiftRight",
];

/// KeyboardEvent.code names for the keys a binding can use.
const CODES: &[(KeyCode, &str)] = &[
    (KeyCode::A, "KeyA"),
    (KeyCode::B, "KeyB"),
    (KeyCode::C, "KeyC"),
    (KeyCode::D, "KeyD"),
    (KeyCode::E, "KeyE"),
    (KeyCode::F, "KeyF"),
    (KeyCode::G, "KeyG"),
    (KeyCode::H, "KeyH"),
    (KeyCode::I, "KeyI"),
    (KeyCode::J, "KeyJ"),
    (KeyCode::K, "KeyK"),
    (KeyCode::L, "KeyL"),
    (KeyCode::M, "KeyM"),
    (KeyCode::N, "KeyN"),
    (KeyCode::O, "KeyO"),
    (KeyCode::P, "KeyP"),
    (KeyCode::Q, "KeyQ"),
    (KeyCode::R, "KeyR"),
    (KeyCode::S, "KeyS"),
    (KeyCode::T, "KeyT"),
    (KeyCode::U, "KeyU"),
    (KeyCode::V, "KeyV"),
    (KeyCode::W, "KeyW"),
    (KeyCode::X, "KeyX"),
    (KeyCode::Y, "KeyY"),
    (KeyCode::Z, "KeyZ"),
    (KeyCode::Key0, "Digit0"),
    (KeyCode::Key1, "Digit1"),
    (KeyCode::Key2, "Digit2"),
    (KeyCode::Key3, "Digit3"),
    (KeyCode::Key4, "Digit4"),
    (KeyCode::Key5, "Digit5"),
    (KeyCode::Key6, "Digit6"),
    (KeyCode::Key7, "Digit7"),
    (KeyCode::Key8, "Digit8"),
    (KeyCode::Key9, "Digit9"),
    (KeyCode::Space, "Space"),
    (KeyCode::Up, "ArrowUp"),
    (KeyCode::Down, "ArrowDown"),
    (KeyCode::Left, "ArrowLeft"),
    (KeyCode::Right, "ArrowRight"),
    (KeyCode::Comma, "Comma"),
    (KeyCode::Period, "Period"),
    (KeyCode::Slash, "Slash"),
    (KeyCode::Semicolon, "Semicolon"),
    (KeyCode::Apostrophe, "Quote"),
    (KeyCode::LeftBracket, "BracketLeft"),
    (KeyCode::RightBracket, "BracketRight"),
    (KeyCode::Backslash, "Backslash"),
    (KeyCode::Minus, "Minus"),
    (KeyCode::Equal, "Equal"),
    (KeyCode::GraveAccent, "Backquote"),
    (KeyCode::Tab, "Tab"),
    (KeyCode::Enter, "Enter"),
    (KeyCode::Escape, "Escape"),
    (KeyCode::LeftShift, "ShiftLeft"),
    (KeyCode::RightShift, "ShiftRight"),
    (KeyCode::LeftControl, "ControlLeft"),
    (KeyCode::RightControl, "ControlRight"),
    (KeyCode::LeftAlt, "AltLeft"),
    (KeyCode::RightAlt, "AltRight"),
    (KeyCode::Kp0, "Numpad0"),
    (KeyCode::Kp1, "Numpad1"),
    (KeyCode::Kp2, "Numpad2"),
    (KeyCode::Kp3, "Numpad3"),
    (KeyCode::Kp4, "Numpad4"),
    (KeyCode::Kp5, "Numpad5"),
    (KeyCode::Kp6, "Numpad6"),
    (KeyCode::Kp7, "Numpad7"),
    (KeyCode::Kp8, "Numpad8"),
    (KeyCode::Kp9, "Numpad9"),
    (KeyCode::KpEnter, "NumpadEnter"),
];

pub fn code_of(k: KeyCode) -> Option<&'static str> {
    CODES.iter().find(|c| c.0 == k).map(|c| c.1)
}

pub fn key_of(code: &str) -> Option<KeyCode> {
    CODES.iter().find(|c| c.1 == code).map(|c| c.0)
}

/// keyLabel: KeyW → W, ArrowUp → ↑, Numpad8 → Num 8.
pub fn key_label(code: &str) -> String {
    match code {
        "ArrowUp" => "↑".to_owned(),
        "ArrowDown" => "↓".to_owned(),
        "ArrowLeft" => "←".to_owned(),
        "ArrowRight" => "→".to_owned(),
        _ => {
            let c = code
                .strip_prefix("Key")
                .or_else(|| code.strip_prefix("Digit"))
                .unwrap_or(code);
            match c.strip_prefix("Numpad") {
                Some(n) => format!("Num {n}"),
                None => c.to_owned(),
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub music: bool,
    pub reduce_motion: bool,
    pub colorblind: bool,
    pub coop: Coop,
    pub voice: bool,
    /// Primary-key overrides by action, as codes.
    pub keys: Vec<(Bind, String)>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            music: true,
            reduce_motion: false,
            colorblind: false,
            coop: Coop::Off,
            voice: true,
            keys: Vec::new(),
        }
    }
}

impl Settings {
    /// Settings.load: every field type-checked; key codes must be plain,
    /// known to this build, and not reserved.
    pub fn from_json(text: Option<&str>) -> Settings {
        let mut s = Settings::default();
        let Some(Value::Object(raw)) = text.and_then(|t| serde_json::from_str::<Value>(t).ok())
        else {
            return s;
        };
        let flag = |k: &str| raw.get(k).and_then(Value::as_bool);
        s.music = flag("music").unwrap_or(s.music);
        s.reduce_motion = flag("reduceMotion").unwrap_or(s.reduce_motion);
        s.colorblind = flag("colorblind").unwrap_or(s.colorblind);
        s.voice = flag("voice").unwrap_or(s.voice);
        if let Some(c) = raw
            .get("coop")
            .and_then(Value::as_str)
            .and_then(Coop::from_key)
        {
            s.coop = c;
        }
        if let Some(Value::Object(keys)) = raw.get("keys") {
            for (b, id, _, _) in KEY_ACTIONS {
                let ok = keys.get(id).and_then(Value::as_str).filter(|c| {
                    c.len() <= 20
                        && c.chars().all(|ch| ch.is_ascii_alphanumeric())
                        && !RESERVED.contains(c)
                        && key_of(c).is_some()
                });
                if let Some(c) = ok {
                    s.keys.push((b, c.to_owned()));
                }
            }
        }
        s.free_new_defaults();
        s
    }

    /// A key the player saved before an action existed wins over that
    /// action's new default (B was free to rebind before PvP took it), so
    /// the newer action moves to the first free letter rather than share
    /// one key between two actions.
    fn free_new_defaults(&mut self) {
        for (b, _, _, d) in KEY_ACTIONS {
            let taken = |s: &Settings, code: &str| {
                KEY_ACTIONS
                    .iter()
                    .any(|a| a.0 != b && s.codes_for(a.0).iter().any(|c| c == code))
            };
            if self.keys.iter().any(|k| k.0 == b) || !taken(self, d[0]) {
                continue;
            }
            let free = CODES
                .iter()
                .map(|c| c.1)
                .find(|c| c.starts_with("Key") && !RESERVED.contains(c) && !taken(self, c));
            if let Some(code) = free {
                self.keys.push((b, code.to_owned()));
            }
        }
    }

    pub fn to_json(&self) -> String {
        let keys: Map<String, Value> = self
            .keys
            .iter()
            .map(|(b, c)| (def(*b).1.to_owned(), Value::from(c.as_str())))
            .collect();
        json!({
            "music": self.music,
            "reduceMotion": self.reduce_motion,
            "colorblind": self.colorblind,
            "coop": self.coop.key(),
            "voice": self.voice,
            "keys": keys,
        })
        .to_string()
    }

    /// codesFor: the codes bound to an action, primary first.
    pub fn codes_for(&self, b: Bind) -> Vec<String> {
        let d = def(b).3;
        let primary = self
            .keys
            .iter()
            .find(|k| k.0 == b)
            .map_or(d[0].to_owned(), |k| k.1.clone());
        std::iter::once(primary)
            .chain(d[1..].iter().map(|c| (*c).to_owned()))
            .collect()
    }

    /// The keys held for an action, as macroquad key codes.
    pub fn keys_for(&self, b: Bind) -> Vec<KeyCode> {
        self.codes_for(b).iter().filter_map(|c| key_of(c)).collect()
    }

    fn set_key(&mut self, b: Bind, code: String) {
        self.keys.retain(|k| k.0 != b);
        self.keys.push((b, code));
    }

    /// finishRebind: Escape cancels, reserved keys and other actions' fixed
    /// keys are refused, and an action already on that key takes this
    /// one's old key.
    pub fn rebind(&mut self, b: Bind, code: &str) -> String {
        let label = def(b).2;
        if code == "Escape" {
            return "Cancelled.".to_owned();
        }
        if RESERVED.contains(&code) {
            return format!("{} is reserved. Pick another key.", key_label(code));
        }
        if let Some(a) = KEY_ACTIONS
            .iter()
            .find(|a| a.0 != b && a.3[1..].contains(&code))
        {
            return format!(
                "{} is a fixed key for {}. Pick another key.",
                key_label(code),
                a.2
            );
        }
        let old = self.codes_for(b)[0].clone();
        let other = KEY_ACTIONS
            .iter()
            .find(|a| a.0 != b && self.codes_for(a.0)[0] == code)
            .map(|a| (a.0, a.2));
        if let Some((o, _)) = other {
            self.set_key(o, old);
        }
        self.set_key(b, code.to_owned());
        match other {
            Some((_, olabel)) => format!(
                "{label} is now {}; {olabel} took the old key.",
                key_label(code)
            ),
            None => format!("{label} is now {}.", key_label(code)),
        }
    }
}

pub fn def(b: Bind) -> (Bind, &'static str, &'static str, &'static [&'static str]) {
    *KEY_ACTIONS.iter().find(|a| a.0 == b).expect("listed")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_load_validates_and_round_trips() {
        let s = Settings::from_json(Some(
            r#"{"music":false,"reduceMotion":true,"colorblind":"yes","coop":"pad",
                "keys":{"thrust":"KeyI","fire":"KeyP","hook":"Bogus!","cut":"Numpad0"}}"#,
        ));
        assert!(!s.music && s.reduce_motion && !s.colorblind);
        assert_eq!(s.coop, Coop::Pad);
        // P is reserved and "Bogus!" isn't a key: both are dropped.
        assert_eq!(s.codes_for(Bind::Thrust), vec!["KeyI", "ArrowUp"]);
        assert_eq!(s.codes_for(Bind::Fire), vec!["Space", "KeyJ"]);
        assert_eq!(s.codes_for(Bind::Hook), vec!["KeyE", "KeyK"]);
        assert_eq!(s.codes_for(Bind::Cut), vec!["Numpad0"]);
        assert_eq!(Settings::from_json(Some(&s.to_json())), s);
        assert_eq!(Settings::from_json(Some("[1]")), Settings::default());
    }

    /// A 1.3.0 save that put Fire on B keeps it; PvP, new in 1.4.0 with B
    /// as its default, moves to a free key instead of sharing B.
    #[test]
    fn a_key_saved_before_pvp_keeps_b_and_pvp_moves() {
        let s = Settings::from_json(Some(r#"{"keys":{"fire":"KeyB"}}"#));
        assert_eq!(s.codes_for(Bind::Fire)[0], "KeyB");
        let pvp = s.codes_for(Bind::Pvp)[0].clone();
        assert_ne!(pvp, "KeyB");
        for (b, ..) in KEY_ACTIONS.iter().filter(|a| a.0 != Bind::Pvp) {
            assert!(!s.codes_for(*b).contains(&pvp), "{pvp} is free");
        }
        assert_eq!(Settings::from_json(Some(&s.to_json())), s, "and it sticks");
        let fresh = Settings::from_json(Some("{}"));
        assert_eq!(fresh.codes_for(Bind::Pvp), vec!["KeyB"], "B by default");
    }

    #[test]
    fn rebinding_swaps_a_taken_key_and_refuses_reserved_ones() {
        let mut s = Settings::default();
        assert_eq!(
            s.rebind(Bind::Fire, "KeyP"),
            "P is reserved. Pick another key."
        );
        assert_eq!(s.rebind(Bind::Fire, "Escape"), "Cancelled.");
        let msg = s.rebind(Bind::Fire, "KeyE");
        assert_eq!(msg, "Fire is now E; Hook took the old key.");
        assert_eq!(s.codes_for(Bind::Fire)[0], "KeyE");
        assert_eq!(s.codes_for(Bind::Hook)[0], "Space");
        assert_eq!(key_label("Numpad8"), "Num 8");
        assert_eq!(key_label("ArrowLeft"), "←");
    }

    #[test]
    fn another_actions_fixed_key_cannot_be_taken() {
        let mut s = Settings::default();
        // J is Fire's fixed alternate: on Hook too, it would do both.
        assert_eq!(
            s.rebind(Bind::Hook, "KeyJ"),
            "J is a fixed key for Fire. Pick another key."
        );
        assert_eq!(s.codes_for(Bind::Hook)[0], "KeyE");
        // An action may still use its own fixed key as its main one.
        assert_eq!(s.rebind(Bind::Fire, "KeyJ"), "Fire is now J.");
    }
}
