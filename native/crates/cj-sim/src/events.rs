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
    Wreck,
    Jumper,
    Crab,
    Reactor,
    Collector,
    Law,
    Miner,
    Muted,
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
    /// A cannon report; the value pans it (-1 left .. 1 right).
    Shoot(i8),
    /// A rival bolt fired this many pixels from the tug (quieter when far).
    EnemyShot(u16),
    /// A rival bolt striking the tug.
    ZapHit,
    /// A latched crab chewing the hull.
    Chomp,
    /// Low-hull alarm beep.
    Alarm,
    /// The under-attack klaxon.
    AttackAlarm,
    /// A grinder slug fired.
    Slug,
    /// The grinder chewing a piece into slugs.
    Grind,
    /// Refinery countdown beep: seconds left (pitch rises as it nears zero).
    ArmBeep(u8),
    JawsWake,
    /// Debt collector arrival siren.
    Siren,
    NetFire,
    Netted,
    /// The netted warble, repeated while caught.
    NetAlarm,
    /// Each tug against the net.
    Rip,
    TornFree,
    /// Impounded: the sad trombone.
    SadTrombone,
    /// Load confiscated (or jacked).
    Confiscated,
    WantedUp,
    /// A police siren this many pixels from the tug.
    PoliceSiren(u16),
    /// A sheriff's stun bolt fired this many pixels away.
    StunShot(u16),
    Arrest,
    /// Missile launch and impact.
    Missile,
    MissileHit,
    SentryShot,
    /// A miner's cutter on your hull.
    BeamZap,
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
    Overheat,
    Reactor,
    Boom,
    Pod,
    NewWreck,
    Crab,
    CrabHull,
    CrabOff,
    Jumper,
    Hunter,
    Latched,
    Severed,
    Escaped,
    Kill,
    Smash,
    Scatter,
    RivalsAnnoyed,
    RivalsHostile,
    RivalsVendetta,
    Grind,
    GrindReactor,
    Scooped,
    EngineOverheat,
    Refinery,
    RefineryHint,
    Drones,
    DroneKill,
    Seated,
    Armed,
    Vault,
    Jaws,
    JawsWrecked,
    JawsShip,
    AteCargo,
    Collector,
    CollectorLeaves,
    CollectorKill,
    Greedy,
    Netted,
    NetEdge,
    NetFree,
    Jacked,
    Confiscated,
    LowWallet,
    FatCable,
    WantedMore,
    Swat,
    SheriffPatrol,
    SheriffWanted,
    Reinforcements,
    Pursuit,
    Shakedown,
    LostSheriff,
    SheriffShotAt,
    SheriffKill,
    Arrested,
    ShookDown,
    Sanctuary,
    MissilesLive,
    SentriesRogue,
    SentryLost,
    RogueDown,
    Miners,
    MinerAngry,
    MinerKill,
    Storm,
    Boss,
    BossDown,
    IonStorm,
    IonClear,
    Derelict,
    Escort,
    EscortDone,
    EscortLow,
    EscortLost,
    HoldOpen,
    HoldSealed,
    CoreZap,
    Training,
    P2Latch,
    P2Free,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Sfx(Sfx),
    /// A radio line. `force` interrupts the cooldown; `once` plays once per
    /// shift. `text` is a line built from this moment's numbers, as (shown,
    /// spoken); without it the game picks one of the key's stock lines.
    Radio {
        key: Radio,
        force: bool,
        once: bool,
        text: Option<(String, String)>,
    },
    /// Freeze the simulation for this many real seconds (hitStop).
    HitStop(f64),
    /// Run the simulation slowly for this many real seconds (slowMo).
    SlowMo(f64),
    /// Something worth a replay happened (noteHighlight).
    Highlight {
        score: f64,
        text: String,
    },
    /// An achievement was earned mid-shift (Profile.unlock), by its id.
    Unlock(&'static str),
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
    /// A side thruster firing on a tug at (x, y) moving at (vx, vy):
    /// (px, py) is its starboard direction, `side` which way the puff
    /// leaves (-1 port, 1 starboard).
    Strafe {
        x: f64,
        y: f64,
        vx: f64,
        vy: f64,
        px: f64,
        py: f64,
        side: f64,
    },
    /// A round striking something: a short flash where it hit.
    Hit {
        x: f64,
        y: f64,
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
