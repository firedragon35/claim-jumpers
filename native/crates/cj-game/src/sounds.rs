//! The sound test (SOUND_TEST in src/js/18-hud-and-screens.js): every
//! sound in the game by group, each a short scripted sequence of cues.

use crate::audio::Audio;
use crate::marge::Voice;
use cj_sim::events::Sfx;

#[derive(Clone, Copy)]
pub enum Cue {
    Sfx(Sfx),
    /// Engine loop level, load, sputtering.
    Thrust(f64, f64, bool),
    Blip,
    Caution,
    /// A Marge line: text, callout, aside.
    Say(&'static str, Option<&'static str>, Option<&'static str>),
    /// Light item `i` (flat index) and name it as now playing.
    Mark(usize),
    /// "Play all" finished.
    Done,
}

pub struct Item {
    pub label: &'static str,
    /// Seconds the item takes ("Play all" waits this long).
    pub dur: f64,
    pub cues: Vec<(f64, Cue)>,
}

/// `n` repeats of a cue every `gap` seconds.
fn every(n: u32, gap: f64, f: impl Fn(u32) -> Cue) -> Vec<(f64, Cue)> {
    (0..n).map(|i| (f64::from(i) * gap, f(i))).collect()
}

fn one(c: Cue) -> Vec<(f64, Cue)> {
    vec![(0.0, c)]
}

pub fn groups() -> Vec<(&'static str, Vec<Item>)> {
    let it = |label, dur, cues| Item { label, dur, cues };
    vec![
        (
            "Weapons",
            vec![
                it("Cutter \"pew\"", 0.6, one(Cue::Sfx(Sfx::Shoot(1)))),
                it(
                    "Cutter, held",
                    1.3,
                    every(8, 0.13, |_| Cue::Sfx(Sfx::Shoot(1))),
                ),
                it(
                    "Twin cutter \"pew-phew\"",
                    1.3,
                    every(8, 0.065, |i| {
                        Cue::Sfx(Sfx::Shoot(if i % 2 == 1 { -1 } else { 1 }))
                    }),
                ),
                it("Grinder slug", 0.7, one(Cue::Sfx(Sfx::Slug))),
                it("Salvage grinder", 0.8, one(Cue::Sfx(Sfx::Grind))),
                it("Missile launch", 0.7, one(Cue::Sfx(Sfx::Missile))),
                it("Missile hit", 0.7, one(Cue::Sfx(Sfx::MissileHit))),
                it(
                    "Sentry drones firing",
                    1.0,
                    every(5, 0.16, |_| Cue::Sfx(Sfx::SentryShot)),
                ),
                it("Enemy blaster", 0.7, one(Cue::Sfx(Sfx::EnemyShot(150)))),
            ],
        ),
        (
            "Taking damage",
            vec![
                it("Under attack alarm", 1.1, one(Cue::Sfx(Sfx::AttackAlarm))),
                it("Hull hit \"bwow\"", 0.7, one(Cue::Sfx(Sfx::Hurt))),
                it("Bolt zap", 0.6, one(Cue::Sfx(Sfx::ZapHit))),
                it("Shield absorbs", 0.6, one(Cue::Sfx(Sfx::ShieldHit))),
                it("Shield down", 1.0, one(Cue::Sfx(Sfx::ShieldDown))),
                it("Shield recharged", 0.6, one(Cue::Sfx(Sfx::ShieldUp))),
                it(
                    "Low-hull alarm",
                    2.0,
                    every(2, 0.9, |_| Cue::Sfx(Sfx::Alarm)),
                ),
                it(
                    "Crab chewing",
                    1.4,
                    every(4, 0.35, |_| Cue::Sfx(Sfx::Chomp)),
                ),
                it(
                    "Miner beam on you",
                    1.0,
                    every(6, 0.13, |_| Cue::Sfx(Sfx::BeamZap)),
                ),
            ],
        ),
        (
            "Nets and debt collectors",
            vec![
                it("Collector siren", 1.0, one(Cue::Sfx(Sfx::Siren))),
                it("Net launch \"fwoomp\"", 0.7, one(Cue::Sfx(Sfx::NetFire))),
                it("Netted \"sproing, uh-oh\"", 1.3, one(Cue::Sfx(Sfx::Netted))),
                it(
                    "Netted warble",
                    2.2,
                    every(2, 1.0, |_| Cue::Sfx(Sfx::NetAlarm)),
                ),
                it(
                    "Tearing the net",
                    1.2,
                    every(5, 0.2, |_| Cue::Sfx(Sfx::Rip)),
                ),
                it("Torn free", 0.8, one(Cue::Sfx(Sfx::TornFree))),
                it("Load confiscated", 1.0, one(Cue::Sfx(Sfx::Confiscated))),
                it(
                    "Impounded (sad trombone)",
                    2.2,
                    one(Cue::Sfx(Sfx::SadTrombone)),
                ),
            ],
        ),
        (
            "The law",
            vec![
                it("Wanted level up", 0.8, one(Cue::Sfx(Sfx::WantedUp))),
                it(
                    "Police siren",
                    1.6,
                    every(2, 0.75, |_| Cue::Sfx(Sfx::PoliceSiren(200))),
                ),
                it("Stun bolt", 0.5, one(Cue::Sfx(Sfx::StunShot(200)))),
                it("Arrested", 1.6, one(Cue::Sfx(Sfx::Arrest))),
            ],
        ),
        (
            "Salvage and money",
            vec![
                it("Hook \"bloop\"", 0.5, one(Cue::Sfx(Sfx::Hook))),
                it("Drop / throw", 0.5, one(Cue::Sfx(Sfx::Drop))),
                it(
                    "Cargo rattle",
                    0.6,
                    every(3, 0.14, |_| Cue::Sfx(Sfx::Rattle)),
                ),
                it("Bank coin", 0.7, one(Cue::Sfx(Sfx::Bank))),
                it("Rare find jackpot", 1.0, one(Cue::Sfx(Sfx::Jackpot))),
                it("Can't do that", 0.5, one(Cue::Sfx(Sfx::Deny))),
            ],
        ),
        (
            "Field and explosions",
            vec![
                it("Rock chip", 0.4, one(Cue::Sfx(Sfx::Chip))),
                it("Rock breaks", 0.6, one(Cue::Sfx(Sfx::Crunch))),
                it("Explosion", 1.2, one(Cue::Sfx(Sfx::Boom))),
                it(
                    "Charge countdown",
                    5.2,
                    every(5, 1.0, |i| Cue::Sfx(Sfx::ArmBeep(5 - i as u8))),
                ),
                it("Refinery jaws wake", 1.6, one(Cue::Sfx(Sfx::JawsWake))),
                it("Engine (light, then loaded)", 3.2, {
                    let mut v = vec![(0.0, Cue::Thrust(1.0, 0.0, false))];
                    v.extend(
                        (0..10).map(|i| {
                            (1.4 + f64::from(i) * 0.15, Cue::Thrust(1.0, 0.9, i % 3 == 0))
                        }),
                    );
                    v.push((3.0, Cue::Thrust(0.0, 0.0, false)));
                    v
                }),
            ],
        ),
        (
            "Radio and Marge",
            vec![
                it("Radio blip", 0.4, one(Cue::Blip)),
                it("Caution chime", 0.6, one(Cue::Caution)),
                it(
                    "Marge: a warning",
                    5.0,
                    one(Cue::Say(
                        "They got a net on you. Wiggle out before they tow you off the field.",
                        Some("Warning. Net. Net."),
                        None,
                    )),
                ),
                it(
                    "Marge: routine line",
                    5.0,
                    one(Cue::Say(
                        "Into the bay it goes. Slowly. Like my pension.",
                        None,
                        Some("Hmph."),
                    )),
                ),
                it(
                    "Marge: rare find",
                    5.0,
                    one(Cue::Say(
                        "A fusion drive. 1,500 cr. Well, well. I am almost impressed. Almost.",
                        None,
                        None,
                    )),
                ),
            ],
        ),
    ]
}

/// Cues waiting their turn, in seconds of the steady clock.
#[derive(Default)]
pub struct Bench {
    queue: Vec<(f64, Cue)>,
    pub now_playing: String,
    /// Which item (flat index) is lit, and until when.
    pub lit: Option<(usize, f64)>,
}

impl Bench {
    /// Queue one item starting `delay` seconds from `now`.
    pub fn play(&mut self, items: &[&Item], i: usize, now: f64, delay: f64) {
        self.queue.push((now + delay, Cue::Mark(i)));
        for (t, c) in &items[i].cues {
            self.queue.push((now + delay + t, *c));
        }
    }

    /// soundTestPlayAll: every item in order, each after the last ends.
    pub fn play_all(&mut self, items: &[&Item], now: f64) {
        let mut t = 0.0;
        for i in 0..items.len() {
            self.play(items, i, now, t);
            t += items[i].dur;
        }
        self.queue.push((now + t, Cue::Done));
    }

    pub fn stop(&mut self, audio: &Audio, voice: &mut Voice) {
        self.queue.clear();
        self.now_playing.clear();
        self.lit = None;
        audio.set_thrust(0.0, 0.0, false);
        voice.stop();
    }

    /// Fire every cue that is due. The sound test speaks even with speech
    /// otherwise limited to play, as the browser's Voice.say allows.
    pub fn update(&mut self, now: f64, items: &[&Item], audio: &mut Audio, voice: &mut Voice) {
        let due: Vec<(f64, Cue)> = self.queue.iter().filter(|q| q.0 <= now).copied().collect();
        self.queue.retain(|q| q.0 > now);
        for (at, c) in due {
            match c {
                Cue::Mark(i) => {
                    self.lit = Some((i, at + items[i].dur));
                    self.now_playing = items[i].label.to_owned();
                }
                Cue::Done => self.now_playing = "Done.".to_owned(),
                Cue::Sfx(s) => audio.sfx(s),
                Cue::Thrust(l, load, sp) => audio.set_thrust(l, load, sp),
                Cue::Blip => audio.radio_blip(),
                Cue::Caution => audio.caution(),
                Cue::Say(text, callout, aside) => voice.say(text, true, callout, aside, audio),
            }
        }
        if self.lit.is_some_and(|l| now > l.1) {
            self.lit = None;
        }
    }
}
