//! Procedural sound, the native counterpart of src/js/03-audio.js.
//!
//! The browser builds every sound from WebAudio oscillators, filtered
//! white noise and gain envelopes; there are no audio files. This module
//! does the same on a cpal output stream (WASAPI on Windows):
//!
//! * `tone` is one enveloped oscillator note with an optional exponential
//!   pitch slide and vibrato, like `Sound.tone`.
//! * `noise` is a white-noise burst through a biquad filter whose cutoff
//!   can sweep, like `Sound.noise`.
//! * The engine is a looping low-passed noise whose gain and cutoff glide
//!   toward targets, like `Sound.setThrust`.
//! * The mix runs through a master gain and a peak compressor with the
//!   browser's DynamicsCompressor settings, so stacked loud effects squash
//!   instead of clipping.
//!
//! The game thread sends commands over a channel; the audio callback never
//! blocks on the game. If no output device can be opened the game runs
//! silently, as the browser does without WebAudio.

use cj_sim::events::Sfx;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::f32::consts::TAU;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

/// Master volume and the per-effect gain (MASTER_VOLUME, SFX_GAIN).
const MASTER_VOLUME: f32 = 0.95;
const SFX_GAIN: f32 = 1.9;
/// Envelopes start and end here rather than at 0, because an exponential
/// ramp cannot reach zero (WebAudio's 0.0001 floor).
const FLOOR: f32 = 0.0001;
/// Seconds between attempts to reopen a lost sound device.
const REOPEN_SECONDS: f64 = 3.0;

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Wave {
    Sine,
    Square,
    Triangle,
    Sawtooth,
}

#[derive(Clone, Copy, PartialEq)]
enum Filter {
    Lowpass,
    Bandpass,
    Highpass,
}

/// Options for one tone, defaulting as `Sound.tone` does.
#[derive(Clone, Copy)]
struct ToneOpts {
    wave: Wave,
    vol: f32,
    /// End pitch as a multiple of the start (0 = no slide).
    slide: f32,
    delay: f32,
    attack: f32,
    /// Vibrato depth in Hz, at `vib_rate` wobbles per second.
    vib: f32,
    vib_rate: f32,
}

const TONE: ToneOpts = ToneOpts {
    wave: Wave::Square,
    vol: 0.12,
    slide: 0.0,
    delay: 0.0,
    attack: 0.004,
    vib: 0.0,
    vib_rate: 0.0,
};

#[derive(Clone, Copy)]
struct NoiseOpts {
    vol: f32,
    freq: f32,
    filter: Filter,
    delay: f32,
    /// Cutoff the filter sweeps to over the burst (0 = fixed).
    sweep_to: f32,
}

const NOISE: NoiseOpts = NoiseOpts {
    vol: 0.25,
    freq: 1200.0,
    filter: Filter::Lowpass,
    delay: 0.0,
    sweep_to: 0.0,
};

/// A sounding voice, in seconds since it was queued. `music` voices play
/// on the music bus (its own gain, no effects gain), as the browser routes
/// the sequencer through Music.gain.
enum Voice {
    Tone {
        freq: f32,
        dur: f32,
        o: ToneOpts,
        t: f32,
        phase: f32,
        lfo: f32,
        music: bool,
        /// Low-pass cutoff in Hz (0 = none), and its filter.
        cut: f32,
        bq: Biquad,
    },
    Noise {
        dur: f32,
        o: NoiseOpts,
        t: f32,
        bq: Biquad,
        music: bool,
    },
}

enum Cmd {
    Play(Voice),
    /// Engine loop targets: gain and low-pass cutoff in Hz.
    Thrust(f32, f32),
    Mute(bool),
    /// Music bus target gain (glides with a 0.3 s time constant).
    MusicGain(f32),
}

/// RBJ cookbook biquad, configured the way WebAudio's BiquadFilterNode is
/// with its default Q of 1 (in dB for low- and high-pass, linear for
/// band-pass).
#[derive(Clone, Copy, Default)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl Biquad {
    fn set(&mut self, kind: Filter, freq: f32, sr: f32) {
        let w0 = TAU * (freq / sr).clamp(1e-4, 0.49);
        let (sn, cs) = w0.sin_cos();
        let (b0, b1, b2, a0, a1, a2) = match kind {
            Filter::Lowpass | Filter::Highpass => {
                // Q = 1 dB of resonance.
                let alpha = sn / (2.0 * 10f32.powf(1.0 / 20.0));
                let a = (1.0 + alpha, -2.0 * cs, 1.0 - alpha);
                if kind == Filter::Lowpass {
                    ((1.0 - cs) / 2.0, 1.0 - cs, (1.0 - cs) / 2.0, a.0, a.1, a.2)
                } else {
                    (
                        (1.0 + cs) / 2.0,
                        -(1.0 + cs),
                        (1.0 + cs) / 2.0,
                        a.0,
                        a.1,
                        a.2,
                    )
                }
            }
            Filter::Bandpass => {
                let alpha = sn / 2.0; // Q = 1
                (alpha, 0.0, -alpha, 1.0 + alpha, -2.0 * cs, 1.0 - alpha)
            }
        };
        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = a1 / a0;
        self.a2 = a2 / a0;
    }

    fn run(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// PolyBLEP step correction, so square and sawtooth edges do not alias
/// harshly (WebAudio's oscillators are band-limited too).
fn blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let t = t / dt;
        t + t - t * t - 1.0
    } else if t > 1.0 - dt {
        let t = (t - 1.0) / dt;
        t * t + t + t + 1.0
    } else {
        0.0
    }
}

fn osc(wave: Wave, phase: f32, dt: f32) -> f32 {
    match wave {
        Wave::Sine => (phase * TAU).sin(),
        Wave::Triangle => 1.0 - 4.0 * (phase - 0.5).abs(),
        Wave::Sawtooth => 2.0 * phase - 1.0 - blep(phase, dt),
        Wave::Square => {
            let s = if phase < 0.5 { 1.0 } else { -1.0 };
            s + blep(phase, dt) - blep((phase + 0.5).fract(), dt)
        }
    }
}

/// An exponential ramp from `a` to `b` at fraction `k` of the way.
fn exp_ramp(a: f32, b: f32, k: f32) -> f32 {
    a * (b / a).powf(k.clamp(0.0, 1.0))
}

/// Small xorshift noise source for the audio thread.
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

/// Everything the audio callback owns.
struct Mixer {
    sr: f32,
    rx: Receiver<Cmd>,
    voices: Vec<Voice>,
    noise: Noise,
    muted: bool,
    // Engine loop: current and target gain and cutoff, and its filter.
    eng_gain: f32,
    eng_gain_to: f32,
    eng_cut: f32,
    eng_cut_to: f32,
    eng_bq: Biquad,
    eng_n: u32,
    /// Compressor gain reduction envelope, in dB.
    comp_db: f32,
    music_gain: f32,
    music_gain_to: f32,
}

impl Mixer {
    fn sample(&mut self) -> f32 {
        let dt = 1.0 / self.sr;
        let mut s = 0.0;
        let mut m = 0.0;
        for v in &mut self.voices {
            match v {
                Voice::Tone {
                    freq,
                    dur,
                    o,
                    t,
                    phase,
                    lfo,
                    music,
                    cut,
                    bq,
                } => {
                    let lt = *t - o.delay;
                    *t += dt;
                    if lt < 0.0 {
                        continue;
                    }
                    let k = lt / *dur;
                    let mut f = if o.slide != 0.0 {
                        exp_ramp(*freq, (*freq * o.slide).max(20.0), k)
                    } else {
                        *freq
                    };
                    if o.vib != 0.0 {
                        *lfo = (*lfo + o.vib_rate * dt).fract();
                        f += (*lfo * TAU).sin() * o.vib;
                    }
                    let inc = (f / self.sr).max(0.0);
                    *phase = (*phase + inc).fract();
                    // Attack to the peak, then decay to the floor at `dur`.
                    let att = o.attack.min(*dur / 2.0);
                    let vol = if *music {
                        o.vol
                    } else {
                        (o.vol * SFX_GAIN).min(1.0)
                    };
                    let env = if lt < att {
                        exp_ramp(FLOOR, vol, lt / att)
                    } else {
                        exp_ramp(vol, FLOOR, (lt - att) / (*dur - att))
                    };
                    let mut x = osc(o.wave, *phase, inc);
                    if *cut > 0.0 {
                        x = bq.run(x);
                    }
                    if *music {
                        m += x * env;
                    } else {
                        s += x * env;
                    }
                }
                Voice::Noise {
                    dur,
                    o,
                    t,
                    bq,
                    music,
                } => {
                    let lt = *t - o.delay;
                    *t += dt;
                    if lt < 0.0 {
                        continue;
                    }
                    let k = lt / *dur;
                    if o.sweep_to != 0.0 {
                        bq.set(o.filter, exp_ramp(o.freq, o.sweep_to, k), self.sr);
                    }
                    let vol = if *music {
                        o.vol
                    } else {
                        (o.vol * SFX_GAIN).min(1.0)
                    };
                    let x = bq.run(self.noise.next()) * exp_ramp(vol, FLOOR, k);
                    if *music {
                        m += x;
                    } else {
                        s += x;
                    }
                }
            }
        }
        // Voices stop 20 ms after their envelope ends, as in the browser.
        self.voices.retain(|v| match v {
            Voice::Tone { dur, o, t, .. } => *t < o.delay + *dur + 0.02,
            Voice::Noise { dur, o, t, .. } => *t < o.delay + *dur + 0.02,
        });

        // Engine: gain glides with a 40 ms time constant, cutoff with 80 ms.
        self.eng_gain += (self.eng_gain_to - self.eng_gain) * (1.0 - (-dt / 0.04).exp());
        self.eng_cut += (self.eng_cut_to - self.eng_cut) * (1.0 - (-dt / 0.08).exp());
        self.eng_n += 1;
        if self.eng_n.is_multiple_of(32) {
            self.eng_bq
                .set(Filter::Lowpass, self.eng_cut.max(20.0), self.sr);
        }
        s += self.eng_bq.run(self.noise.next()) * self.eng_gain;
        self.music_gain += (self.music_gain_to - self.music_gain) * (1.0 - (-dt / 0.3).exp());
        s += m * self.music_gain;

        if self.muted {
            return 0.0;
        }
        self.compress(s * MASTER_VOLUME)
    }

    /// DynamicsCompressor(threshold -14 dB, knee 10, ratio 4, attack 3 ms,
    /// release 150 ms), as a simple peak-following compressor.
    fn compress(&mut self, x: f32) -> f32 {
        let (thr, knee, ratio) = (-14.0f32, 10.0f32, 4.0f32);
        let lvl = 20.0 * x.abs().max(1e-6).log10();
        let over = lvl - thr;
        let want = if over <= -knee / 2.0 {
            0.0
        } else if over >= knee / 2.0 {
            over * (1.0 - 1.0 / ratio)
        } else {
            let k = over + knee / 2.0;
            (1.0 - 1.0 / ratio) * k * k / (2.0 * knee)
        };
        let tau = if want > self.comp_db { 0.003 } else { 0.15 };
        self.comp_db += (want - self.comp_db) * (1.0 - (-1.0 / (self.sr * tau)).exp());
        (x * 10f32.powf(-self.comp_db / 20.0)).clamp(-1.0, 1.0)
    }

    fn drain(&mut self) {
        while let Ok(cmd) = self.rx.try_recv() {
            match cmd {
                Cmd::Play(mut v) => {
                    match &mut v {
                        Voice::Noise { o, bq, .. } => bq.set(o.filter, o.freq, self.sr),
                        Voice::Tone { cut, bq, .. } if *cut > 0.0 => {
                            bq.set(Filter::Lowpass, *cut, self.sr);
                        }
                        Voice::Tone { .. } => {}
                    }
                    self.voices.push(v);
                }
                Cmd::MusicGain(g) => self.music_gain_to = g,
                Cmd::Thrust(g, c) => {
                    self.eng_gain_to = g;
                    self.eng_cut_to = c;
                }
                Cmd::Mute(m) => self.muted = m,
            }
        }
    }
}

pub struct Audio {
    tx: Option<Sender<Cmd>>,
    _stream: Option<cpal::Stream>,
    /// Set by the stream's error callback when the device is lost.
    failed: Arc<AtomicBool>,
    /// Clock time of the next attempt to reopen the device.
    reopen_at: f64,
    muted: bool,
    /// Scale on the next effect's volume (1 normally; less for another
    /// player's sound heard from a distance, see `sfx_at`).
    sfx_scale: f32,
    /// Seconds of game time, for rate-limiting the cargo rattle.
    clock: f64,
    last_rattle: f64,
    rng: Noise,
    /// Why audio is off, if it is.
    pub error: Option<String>,
}

impl Audio {
    /// Open the default output device. On failure the game runs silent.
    pub fn start() -> Audio {
        let failed = Arc::new(AtomicBool::new(false));
        let (tx, stream, error) = match open(&failed) {
            Ok((tx, stream)) => (Some(tx), Some(stream), None),
            Err(e) => {
                // Marked as lost, so plugging a device in later brings sound back.
                failed.store(true, Ordering::Relaxed);
                (None, None, Some(e))
            }
        };
        Audio {
            tx,
            _stream: stream,
            failed,
            reopen_at: 0.0,
            muted: false,
            sfx_scale: 1.0,
            clock: 0.0,
            last_rattle: -1.0,
            rng: Noise(0x9e37_79b9),
            error,
        }
    }

    /// Reopen the default device if the stream was lost (a headset
    /// unplugged, say). cpal's stream thread ends on such an error and
    /// never restarts by itself. Call once a frame; `now` is in seconds.
    pub fn recover(&mut self, now: f64) {
        if !self.failed.load(Ordering::Relaxed) || now < self.reopen_at {
            return;
        }
        self.reopen_at = now + REOPEN_SECONDS;
        if let Ok((tx, stream)) = open(&self.failed) {
            self.failed.store(false, Ordering::Relaxed);
            self.tx = Some(tx);
            self._stream = Some(stream);
            self.error = None;
            // A new mixer starts unmuted. Music and engine levels are re-sent anyway.
            self.send(Cmd::Mute(self.muted));
        }
    }

    /// Advance the clock the rattle rate limit is measured on.
    pub fn tick(&mut self, dt: f64) {
        self.clock += dt;
    }

    pub fn toggle_mute(&mut self) {
        self.muted = !self.muted;
        self.send(Cmd::Mute(self.muted));
    }

    pub fn is_muted(&self) -> bool {
        self.muted
    }

    /// The radio's two-tone blip as a line comes in.
    pub fn radio_blip(&self) {
        let o = |delay: f32| ToneOpts {
            vol: 0.03,
            delay,
            ..TONE
        };
        self.tone(1500.0, 0.035, o(0.0));
        self.tone(1100.0, 0.035, o(0.05));
    }

    /// A master-caution chime, played before a spoken callout.
    pub fn caution(&self) {
        for (i, d) in [0.0, 0.16].into_iter().enumerate() {
            let f = if i > 0 { 740.0 } else { 988.0 };
            self.tone(
                f,
                0.13,
                ToneOpts {
                    wave: Wave::Sine,
                    vol: 0.12,
                    delay: d,
                    ..TONE
                },
            );
        }
    }

    fn send(&self, c: Cmd) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(c);
        }
    }

    /// `rand(a, b)` for pitch variation.
    fn rand(&mut self, a: f32, b: f32) -> f32 {
        a + (self.rng.next() * 0.5 + 0.5) * (b - a)
    }

    fn tone(&self, freq: f32, dur: f32, mut o: ToneOpts) {
        if self.muted {
            return;
        }
        o.vol *= self.sfx_scale;
        self.send(Cmd::Play(Voice::Tone {
            freq,
            dur,
            o,
            t: 0.0,
            phase: 0.0,
            lfo: 0.0,
            music: false,
            cut: 0.0,
            bq: Biquad::default(),
        }));
    }

    /// A music note (Music.playStep's note()): a 10 ms attack, an
    /// exponential decay over `len`, optionally low-passed.
    pub(crate) fn note(&self, freq: f32, wave: Wave, delay: f32, len: f32, vol: f32, cutoff: f32) {
        self.send(Cmd::Play(Voice::Tone {
            freq,
            dur: len,
            o: ToneOpts {
                wave,
                vol,
                delay,
                attack: 0.01,
                ..TONE
            },
            t: 0.0,
            phase: 0.0,
            lfo: 0.0,
            music: true,
            cut: cutoff,
            bq: Biquad::default(),
        }));
    }

    /// The music's kick: a sine falling from 130 Hz to 40 Hz.
    // shortcut: the pitch falls over the whole 0.2 s note, where the browser
    // reaches 40 Hz at 0.15 s; upgrade with a separate slide time if it matters.
    pub(crate) fn kick(&self, delay: f32) {
        self.send(Cmd::Play(Voice::Tone {
            freq: 130.0,
            dur: 0.2,
            o: ToneOpts {
                wave: Wave::Sine,
                vol: 0.28,
                slide: 40.0 / 130.0,
                delay,
                attack: 0.001,
                ..TONE
            },
            t: 0.0,
            phase: 0.0,
            lfo: 0.0,
            music: true,
            cut: 0.0,
            bq: Biquad::default(),
        }));
    }

    /// High-passed music noise (hats and snares).
    pub(crate) fn music_noise(&self, delay: f32, len: f32, vol: f32, hp: f32) {
        self.send(Cmd::Play(Voice::Noise {
            dur: len,
            o: NoiseOpts {
                vol,
                freq: hp,
                filter: Filter::Highpass,
                delay,
                sweep_to: 0.0,
            },
            t: 0.0,
            bq: Biquad::default(),
            music: true,
        }));
    }

    /// Where the music bus should settle (Music.gain's target).
    pub fn set_music_gain(&self, g: f32) {
        self.send(Cmd::MusicGain(g));
    }

    fn noise(&self, dur: f32, mut o: NoiseOpts) {
        if self.muted {
            return;
        }
        o.vol *= self.sfx_scale;
        self.send(Cmd::Play(Voice::Noise {
            dur,
            o,
            t: 0.0,
            bq: Biquad::default(),
            music: false,
        }));
    }

    /// The engine loop: heavy loads lower the pitch; sputters cut out.
    pub fn set_thrust(&self, level: f64, load: f64, sputtering: bool) {
        let level = level as f32;
        let g = if self.muted {
            0.0
        } else {
            level * if sputtering { 0.08 } else { 0.32 }
        };
        self.send(Cmd::Thrust(g, 300.0 + level * 520.0 - load as f32 * 220.0));
    }

    /// Play one effect at `gain` times its usual volume (0 to 1): another
    /// player's sound, quieter the further away it is.
    pub fn sfx_at(&mut self, s: Sfx, gain: f32) {
        self.sfx_scale = gain.clamp(0.0, 1.0);
        self.sfx(s);
        self.sfx_scale = 1.0;
    }

    /// Play one effect (Sound.sfx.*).
    pub fn sfx(&mut self, s: Sfx) {
        let t = |w: Wave, vol: f32| ToneOpts {
            wave: w,
            vol,
            ..TONE
        };
        let n = |vol: f32, freq: f32| NoiseOpts { vol, freq, ..NOISE };
        use Wave::*;
        match s {
            // Cutter: a bright square "pew" sweeping down about three
            // octaves over a softer triangle. Twin barrels pitch apart.
            Sfx::Shoot(barrel) => {
                let f = if barrel > 0 { 1550.0 } else { 1380.0 } * self.rand(0.97, 1.03);
                self.tone(
                    f,
                    0.13,
                    ToneOpts {
                        slide: 0.12,
                        ..t(Square, 0.045)
                    },
                );
                self.tone(
                    f / 2.0,
                    0.11,
                    ToneOpts {
                        slide: 0.15,
                        ..t(Triangle, 0.04)
                    },
                );
            }
            // Enemy blaster: lower and buzzier, quieter with distance.
            Sfx::EnemyShot(d) => {
                let d = f32::from(d);
                // At exactly 1100 the volume would be 0, and an exponential
                // fade from 0 produces NaN samples (see exp_ramp).
                if d >= 1100.0 {
                    return;
                }
                let v = 0.06 * (1.0 - d / 1100.0);
                self.tone(
                    900.0,
                    0.18,
                    ToneOpts {
                        slide: 0.12,
                        ..t(Sawtooth, v)
                    },
                );
                self.tone(
                    700.0,
                    0.16,
                    ToneOpts {
                        slide: 0.13,
                        ..t(Square, v * 0.6)
                    },
                );
            }
            Sfx::ZapHit => {
                self.tone(
                    620.0,
                    0.14,
                    ToneOpts {
                        slide: 0.13,
                        ..t(Square, 0.1)
                    },
                );
                self.noise(
                    0.08,
                    NoiseOpts {
                        filter: Filter::Highpass,
                        ..n(0.16, 3000.0)
                    },
                );
            }
            // Grinder slug: a heavier "thoom-pew".
            Sfx::Slug => {
                self.tone(
                    420.0,
                    0.22,
                    ToneOpts {
                        slide: 0.15,
                        ..t(Sawtooth, 0.09)
                    },
                );
                self.noise(0.12, n(0.12, 800.0));
            }
            // The salvage grinder chewing a piece into slugs.
            Sfx::Grind => self.noise(
                0.4,
                NoiseOpts {
                    filter: Filter::Bandpass,
                    ..n(0.3, 1800.0)
                },
            ),
            // Refinery countdown: one beep per second, rising as it nears zero.
            Sfx::ArmBeep(sec) => {
                let f = 600.0 + (5.0 - f32::from(sec)) * 150.0;
                self.tone(f, 0.1, ToneOpts { vol: 0.08, ..TONE });
            }
            // The jaws waking: a long, low groan.
            Sfx::JawsWake => self.tone(
                55.0,
                1.2,
                ToneOpts {
                    slide: 0.7,
                    ..t(Sawtooth, 0.25)
                },
            ),
            // Collector arrival: a three-cycle two-tone siren.
            Sfx::Siren => {
                for i in 0..3 {
                    let d = i as f32 * 0.26;
                    self.tone(
                        620.0,
                        0.12,
                        ToneOpts {
                            vol: 0.05,
                            delay: d,
                            ..TONE
                        },
                    );
                    self.tone(
                        820.0,
                        0.12,
                        ToneOpts {
                            vol: 0.05,
                            delay: d + 0.13,
                            ..TONE
                        },
                    );
                }
            }
            // Net launched: a rising "fwoomp".
            Sfx::NetFire => {
                let o = NoiseOpts {
                    filter: Filter::Bandpass,
                    sweep_to: 2400.0,
                    ..n(0.18, 400.0)
                };
                self.noise(0.3, o);
                self.tone(
                    180.0,
                    0.28,
                    ToneOpts {
                        slide: 3.8,
                        ..t(Triangle, 0.1)
                    },
                );
            }
            // Netted: a wobbly "sproing" and a thwap, then a falling "uh-oh-oh".
            Sfx::Netted => {
                let sproing = ToneOpts {
                    slide: 0.13,
                    vib: 45.0,
                    vib_rate: 22.0,
                    ..t(Triangle, 0.14)
                };
                self.tone(700.0, 0.55, sproing);
                self.noise(0.18, n(0.3, 500.0));
                self.tone(
                    523.0,
                    0.12,
                    ToneOpts {
                        vol: 0.07,
                        delay: 0.3,
                        ..TONE
                    },
                );
                self.tone(
                    392.0,
                    0.12,
                    ToneOpts {
                        vol: 0.07,
                        delay: 0.44,
                        ..TONE
                    },
                );
                let last = ToneOpts {
                    vol: 0.07,
                    delay: 0.58,
                    vib: 8.0,
                    vib_rate: 6.0,
                    ..TONE
                };
                self.tone(311.0, 0.32, last);
            }
            // While netted: a "wee-oo" warble.
            Sfx::NetAlarm => {
                self.tone(880.0, 0.1, ToneOpts { vol: 0.05, ..TONE });
                self.tone(
                    660.0,
                    0.12,
                    ToneOpts {
                        vol: 0.05,
                        delay: 0.11,
                        ..TONE
                    },
                );
            }
            // Each tug against the net: a ripping zip.
            Sfx::Rip => {
                self.noise(
                    0.07,
                    NoiseOpts {
                        filter: Filter::Highpass,
                        ..n(0.18, 2500.0)
                    },
                );
                self.tone(
                    300.0,
                    0.06,
                    ToneOpts {
                        vol: 0.05,
                        slide: 4.0,
                        ..TONE
                    },
                );
            }
            // Torn free: a triumphant rising arpeggio.
            Sfx::TornFree => {
                for (i, f) in [523.0, 659.0, 784.0, 1047.0].into_iter().enumerate() {
                    self.tone(
                        f,
                        0.08,
                        ToneOpts {
                            vol: 0.07,
                            delay: i as f32 * 0.06,
                            ..TONE
                        },
                    );
                }
                self.tone(
                    1568.0,
                    0.25,
                    ToneOpts {
                        delay: 0.24,
                        ..t(Triangle, 0.08)
                    },
                );
            }
            // Impounded: the sad trombone, "wah wah wah waaah".
            Sfx::SadTrombone => {
                for (i, f) in [392.0, 370.0, 349.0].into_iter().enumerate() {
                    let o = ToneOpts {
                        delay: i as f32 * 0.3,
                        attack: 0.03,
                        ..t(Sawtooth, 0.09)
                    };
                    self.tone(f, 0.28, o);
                }
                let o = ToneOpts {
                    delay: 0.9,
                    attack: 0.03,
                    vib: 6.0,
                    vib_rate: 5.0,
                    ..t(Sawtooth, 0.09)
                };
                self.tone(330.0, 0.9, o);
            }
            // Load confiscated: coins falling out of your pocket, then a thunk.
            Sfx::Confiscated => {
                for (i, f) in [1319.0, 1175.0, 988.0, 784.0, 659.0]
                    .into_iter()
                    .enumerate()
                {
                    self.tone(
                        f,
                        0.08,
                        ToneOpts {
                            vol: 0.06,
                            delay: i as f32 * 0.07,
                            ..TONE
                        },
                    );
                }
                self.tone(
                    110.0,
                    0.25,
                    ToneOpts {
                        delay: 0.38,
                        slide: 0.5,
                        ..t(Square, 0.08)
                    },
                );
            }
            // Wanted level rising: an ominous falling triad.
            Sfx::WantedUp => {
                for (i, f) in [659.0, 554.0, 440.0].into_iter().enumerate() {
                    self.tone(
                        f,
                        0.18,
                        ToneOpts {
                            delay: i as f32 * 0.12,
                            ..t(Triangle, 0.08)
                        },
                    );
                }
            }
            // Police siren: a warbling "wee-woo", quieter with distance.
            Sfx::PoliceSiren(d) => {
                let v = 0.06 * (1.2 - f32::from(d) / 1600.0).clamp(0.25, 1.0);
                let o = ToneOpts {
                    vol: v,
                    vib: 20.0,
                    vib_rate: 7.0,
                    ..TONE
                };
                self.tone(960.0, 0.32, o);
                self.tone(740.0, 0.34, ToneOpts { delay: 0.36, ..o });
            }
            // Sheriff stun bolt: an electric "pwang".
            Sfx::StunShot(d) => {
                let d = f32::from(d);
                if d < 1100.0 {
                    let vol = 0.07 * (1.0 - d / 1100.0) + 0.02;
                    let o = ToneOpts {
                        slide: 0.35,
                        vib: 60.0,
                        vib_rate: 30.0,
                        ..t(Sine, vol)
                    };
                    self.tone(1600.0, 0.2, o);
                }
            }
            // Arrested: two cuff clicks and a low "dun-dun".
            Sfx::Arrest => {
                let click = NoiseOpts {
                    filter: Filter::Highpass,
                    ..n(0.25, 4000.0)
                };
                self.noise(0.04, click);
                self.noise(
                    0.04,
                    NoiseOpts {
                        delay: 0.12,
                        ..click
                    },
                );
                self.tone(
                    147.0,
                    0.35,
                    ToneOpts {
                        delay: 0.3,
                        ..t(Sawtooth, 0.1)
                    },
                );
                self.tone(
                    110.0,
                    0.7,
                    ToneOpts {
                        delay: 0.7,
                        ..t(Sawtooth, 0.1)
                    },
                );
            }
            // Missile launch and impact.
            Sfx::Missile => {
                let o = NoiseOpts {
                    filter: Filter::Bandpass,
                    sweep_to: 2500.0,
                    ..n(0.12, 600.0)
                };
                self.noise(0.25, o);
                self.tone(
                    320.0,
                    0.2,
                    ToneOpts {
                        slide: 2.5,
                        ..t(Sawtooth, 0.04)
                    },
                );
            }
            Sfx::MissileHit => {
                self.noise(0.3, n(0.22, 900.0));
                self.tone(
                    200.0,
                    0.2,
                    ToneOpts {
                        slide: 0.3,
                        ..t(Square, 0.06)
                    },
                );
            }
            // Sentry drone shot: a small, high "tik".
            Sfx::SentryShot => self.tone(
                2100.0,
                0.06,
                ToneOpts {
                    slide: 0.4,
                    ..t(Square, 0.025)
                },
            ),
            // Miner's cutter on your hull: a sizzling buzz.
            Sfx::BeamZap => self.tone(
                1300.0,
                0.12,
                ToneOpts {
                    vib: 200.0,
                    vib_rate: 40.0,
                    ..t(Sawtooth, 0.04)
                },
            ),
            Sfx::Chip => self.noise(0.07, n(0.1, 2200.0)),
            Sfx::Crunch => self.noise(0.32, n(0.28, 650.0)),
            Sfx::Boom => {
                self.noise(0.9, n(0.55, 320.0));
                self.tone(
                    90.0,
                    0.7,
                    ToneOpts {
                        slide: 0.35,
                        ..t(Sine, 0.35)
                    },
                );
                self.tone(
                    130.0,
                    0.45,
                    ToneOpts {
                        slide: 0.25,
                        ..t(Square, 0.07)
                    },
                );
            }
            Sfx::Hook => self.tone(
                260.0,
                0.09,
                ToneOpts {
                    slide: 2.4,
                    ..t(Sine, 0.16)
                },
            ),
            Sfx::Drop => self.tone(
                300.0,
                0.12,
                ToneOpts {
                    slide: 0.5,
                    ..t(Triangle, 0.12)
                },
            ),
            Sfx::Bank => self.bank(),
            Sfx::Jackpot => {
                self.bank();
                for (i, f) in [1047.0, 1319.0, 1568.0, 2093.0].into_iter().enumerate() {
                    let delay = 0.14 + i as f32 * 0.06;
                    self.tone(
                        f,
                        0.16,
                        ToneOpts {
                            delay,
                            ..t(Triangle, 0.08)
                        },
                    );
                }
            }
            Sfx::Hurt => {
                self.tone(
                    240.0,
                    0.22,
                    ToneOpts {
                        slide: 0.23,
                        ..t(Square, 0.1)
                    },
                );
                self.noise(0.14, n(0.22, 900.0));
            }
            Sfx::Alarm => {
                self.tone(1050.0, 0.07, t(Square, 0.06));
                self.tone(
                    1050.0,
                    0.07,
                    ToneOpts {
                        delay: 0.12,
                        ..t(Square, 0.06)
                    },
                );
            }
            Sfx::Chomp => {
                self.tone(
                    140.0,
                    0.06,
                    ToneOpts {
                        slide: 0.5,
                        ..t(Square, 0.09)
                    },
                );
                self.noise(0.04, n(0.1, 1500.0));
            }
            Sfx::ShieldHit => {
                self.tone(
                    1800.0,
                    0.16,
                    ToneOpts {
                        slide: 0.6,
                        ..t(Triangle, 0.07)
                    },
                );
                self.noise(
                    0.08,
                    NoiseOpts {
                        filter: Filter::Highpass,
                        ..n(0.08, 5000.0)
                    },
                );
            }
            Sfx::ShieldDown => self.tone(
                900.0,
                0.6,
                ToneOpts {
                    slide: 0.12,
                    vib: 30.0,
                    vib_rate: 14.0,
                    ..t(Sawtooth, 0.08)
                },
            ),
            Sfx::ShieldUp => {
                self.tone(880.0, 0.1, t(Sine, 0.06));
                self.tone(
                    1320.0,
                    0.18,
                    ToneOpts {
                        delay: 0.09,
                        ..t(Sine, 0.06)
                    },
                );
            }
            // "Red alert": two rising whoops.
            Sfx::AttackAlarm => {
                for delay in [0.0, 0.42] {
                    let o = ToneOpts {
                        slide: 2.4,
                        attack: 0.02,
                        delay,
                        ..t(Sawtooth, 0.07)
                    };
                    self.tone(380.0, 0.36, o);
                }
            }
            Sfx::Deny => self.tone(140.0, 0.12, t(Square, 0.07)),
            // Cargo rattling on the cable, limited so it never drones.
            Sfx::Rattle => {
                if self.clock - self.last_rattle < 0.12 {
                    return;
                }
                self.last_rattle = self.clock;
                let freq = self.rand(2500.0, 4500.0);
                self.noise(
                    0.05,
                    NoiseOpts {
                        filter: Filter::Bandpass,
                        ..n(0.07, freq)
                    },
                );
            }
        }
    }

    /// Coin "ba-ding": two square notes a fourth apart.
    fn bank(&self) {
        self.tone(988.0, 0.07, ToneOpts { vol: 0.08, ..TONE });
        self.tone(
            1319.0,
            0.3,
            ToneOpts {
                vol: 0.08,
                delay: 0.07,
                ..TONE
            },
        );
    }
}

/// True if this stream error means the stream has stopped for good. The
/// others are reports about a stream that is still playing.
fn stream_is_lost(kind: cpal::ErrorKind) -> bool {
    use cpal::ErrorKind::{DeviceChanged, RealtimeDenied, Xrun};
    !matches!(kind, Xrun | RealtimeDenied | DeviceChanged)
}

fn open(failed: &Arc<AtomicBool>) -> Result<(Sender<Cmd>, cpal::Stream), String> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or("no default output device")?;
    let supported = device.default_output_config().map_err(|e| e.to_string())?;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let (tx, rx) = channel();
    let mixer = Mixer {
        sr: config.sample_rate as f32,
        rx,
        voices: Vec::new(),
        noise: Noise(0x1234_5678),
        muted: false,
        eng_gain: 0.0,
        eng_gain_to: 0.0,
        eng_cut: 400.0,
        eng_cut_to: 400.0,
        eng_bq: Biquad::default(),
        eng_n: 0,
        comp_db: 0.0,
        music_gain: 0.0,
        music_gain_to: 0.0,
    };
    let stream = match format {
        cpal::SampleFormat::F32 => build::<f32>(&device, &config, mixer, failed.clone()),
        cpal::SampleFormat::I16 => build::<i16>(&device, &config, mixer, failed.clone()),
        cpal::SampleFormat::U16 => build::<u16>(&device, &config, mixer, failed.clone()),
        other => return Err(format!("unsupported sample format {other:?}")),
    }?;
    stream.play().map_err(|e| e.to_string())?;
    Ok((tx, stream))
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut mixer: Mixer,
    failed: Arc<AtomicBool>,
) -> Result<cpal::Stream, String>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    let channels = usize::from(config.channels);
    mixer.eng_bq.set(Filter::Lowpass, 400.0, mixer.sr);
    device
        .build_output_stream(
            *config,
            move |data: &mut [T], _| {
                mixer.drain();
                for frame in data.chunks_mut(channels) {
                    let v = T::from_sample(mixer.sample());
                    for out in frame.iter_mut() {
                        *out = v;
                    }
                }
            },
            move |err: cpal::Error| {
                eprintln!("audio stream error: {err}");
                if stream_is_lost(err.kind()) {
                    failed.store(true, Ordering::Relaxed);
                }
            },
            None,
        )
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_enemy_shot_at_the_edge_of_hearing_is_not_played() {
        let (tx, rx) = channel();
        let mut a = Audio {
            tx: Some(tx),
            _stream: None,
            failed: Arc::new(AtomicBool::new(false)),
            reopen_at: 0.0,
            muted: false,
            sfx_scale: 1.0,
            clock: 0.0,
            last_rattle: -1.0,
            rng: Noise(1),
            error: None,
        };
        // Exactly 1100 is volume 0, which would fade to NaN samples.
        a.sfx(Sfx::EnemyShot(1100));
        assert!(rx.try_recv().is_err());
        a.sfx(Sfx::EnemyShot(1099));
        assert!(rx.try_recv().is_ok());
    }

    #[test]
    fn only_a_stopped_stream_counts_as_lost() {
        assert!(stream_is_lost(cpal::ErrorKind::DeviceNotAvailable));
        assert!(stream_is_lost(cpal::ErrorKind::StreamInvalidated));
        assert!(!stream_is_lost(cpal::ErrorKind::Xrun));
    }

    #[test]
    fn lowpass_passes_dc_and_blocks_nyquist() {
        let mut bq = Biquad::default();
        bq.set(Filter::Lowpass, 400.0, 48_000.0);
        let mut y = 0.0;
        for _ in 0..4800 {
            y = bq.run(1.0);
        }
        assert!((y - 1.0).abs() < 1e-3, "DC gain is 1, got {y}");
        // A fresh filter, measured once its start-up transient has died away.
        let mut bq = Biquad::default();
        bq.set(Filter::Lowpass, 400.0, 48_000.0);
        let mut peak = 0.0f32;
        for i in 0..4800 {
            let x = if i % 2 == 0 { 1.0 } else { -1.0 };
            let y = bq.run(x);
            if i >= 2400 {
                peak = peak.max(y.abs());
            }
        }
        assert!(peak < 0.01, "Nyquist is blocked, got {peak}");
    }

    #[test]
    fn compressor_holds_a_loud_signal_under_full_scale() {
        let (_tx, rx) = channel();
        let mut m = Mixer {
            sr: 48_000.0,
            rx,
            voices: Vec::new(),
            noise: Noise(1),
            muted: false,
            eng_gain: 0.0,
            eng_gain_to: 0.0,
            eng_cut: 400.0,
            eng_cut_to: 400.0,
            eng_bq: Biquad::default(),
            eng_n: 0,
            comp_db: 0.0,
            music_gain: 0.0,
            music_gain_to: 0.0,
        };
        let mut last = 0.0;
        for i in 0..48_000 {
            let x = (i as f32 * 440.0 / 48_000.0 * TAU).sin() * 3.0;
            last = m.compress(x).abs().max(last * 0.999);
        }
        assert!(last <= 1.0);
        assert!(m.comp_db > 6.0, "it is reducing gain: {} dB", m.comp_db);
    }
}
