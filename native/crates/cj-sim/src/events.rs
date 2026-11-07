//! What the simulation tells the presentation layer.
//!
//! The browser plays sounds, spawns particles and has Marge talk straight
//! from gameplay code. Here gameplay only records [`Event`]s; the game crate
//! drains them each frame to drive effects, audio and the radio. Nothing in
//! the simulation depends on how (or whether) events are presented.

/// Palette colours effects refer to (COL in src/js/02-utilities.js).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tint {
    Hazard,
    Cable,
    Steel,
    Rock,
    Ore,
    Danger,
    Text,
    /// An explicit 0xRRGGBB colour (rare item grades carry their own).
    Rgb(u32),
}

/// One-shot sound effects (Sound.sfx in src/js/03-audio.js).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sfx {
    Hook,
    Deny,
    Drop,
    Bank,
    Jackpot,
    Chip,
    Crunch,
    Rattle,
    Hurt,
    ShieldHit,
    ShieldDown,
    ShieldUp,
    Boom,
}

/// Lines Marge can say (the radio keys in src/js/04-dispatcher-radio.js).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Radio {
    Start,
    Hook,
    Heavy,
    Engine,
    Maxed,
    Cut,
    Bank,
    BankBig,
    Rare,
    Repair,
    HullLow,
    ShieldDown,
    Dead,
    Minute,
    TenSec,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Sfx(Sfx),
    /// A radio line. `force` interrupts the cooldown; `once` plays once per shift.
    Radio {
        key: Radio,
        force: bool,
        once: bool,
    },
    /// Sparks flying out from a point.
    Burst {
        x: f64,
        y: f64,
        n: u32,
        tint: Tint,
        speed: f64,
    },
    /// Rock dust: smoke puffs plus chips (dustFx).
    Dust {
        x: f64,
        y: f64,
        r: f64,
        tint: Tint,
    },
    /// Fireball (explosionFx).
    Explosion {
        x: f64,
        y: f64,
        size: f64,
        tint: Tint,
    },
    /// Floating text in the world.
    Popup {
        x: f64,
        y: f64,
        text: String,
        tint: Tint,
    },
    /// Controller rumble: strength 0..1 and duration in milliseconds.
    Rumble {
        strength: f64,
        ms: f64,
    },
    /// The engine loop's state, sent each step while the tug flies.
    Thrust {
        level: f64,
        load: f64,
        sputtering: bool,
    },
}
