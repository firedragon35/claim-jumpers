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
        place: Place,
    },
    Noise {
        dur: f32,
        o: NoiseOpts,
        t: f32,
        bq: Biquad,
        music: bool,
        place: Place,
    },
}

/// Where an effect is heard from: its stereo pan (-1 left .. 1 right) and,
/// for a distant one, a low-pass cutoff in Hz that muffles it (0 = none).
#[derive(Clone, Copy, Default)]
struct Place {
    pan: f32,
    muffle: f32,
    bq: Biquad,
}

impl Place {
    /// Equal-power left and right gains, 1 each when centred.
    fn gains(&self) -> (f32, f32) {
        let a = (self.pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
        (
            a.cos() * std::f32::consts::SQRT_2,
            a.sin() * std::f32::consts::SQRT_2,
        )
    }

    fn run(&mut self, x: f32) -> f32 {
        if self.muffle > 0.0 {
            self.bq.run(x)
        } else {
            x
        }
    }
}

/// A stereo feedback echo on the effects bus: a little in open space, more
/// near the carrier's and the refinery's hulls. The two sides repeat at
/// different times so the tail spreads wide.
struct Echo {
    l: Vec<f32>,
    r: Vec<f32>,
    i: usize,
    dl: usize,
    dr: usize,
    /// Wet level and its target.
    wet: f32,
    wet_to: f32,
    /// One-pole low-pass state on each feedback path: repeats get darker.
    lp: (f32, f32),
}

/// Seconds before each side's first repeat, and how much each repeat keeps.
const ECHO_L: f32 = 0.27;
const ECHO_R: f32 = 0.34;
const ECHO_FEEDBACK: f32 = 0.38;
/// The music's level under a radio line.
const DUCK: f32 = 0.45;

impl Echo {
    fn new(sr: f32) -> Echo {
        let n = (sr * 0.5) as usize;
        Echo {
            l: vec![0.0; n],
            r: vec![0.0; n],
            i: 0,
            dl: ((sr * ECHO_L) as usize).min(n - 1),
            dr: ((sr * ECHO_R) as usize).min(n - 1),
            wet: 0.0,
            wet_to: 0.0,
            lp: (0.0, 0.0),
        }
    }

    /// Feeds (l, r) in and returns the echo to add to the mix.
    fn run(&mut self, l: f32, r: f32, dt: f32) -> (f32, f32) {
        self.wet += (self.wet_to - self.wet) * (1.0 - (-dt / 0.5).exp());
        let n = self.l.len();
        let el = self.l[(self.i + n - self.dl) % n];
        let er = self.r[(self.i + n - self.dr) % n];
        // Repeats cross sides a little and lose their top end.
        self.lp.0 += (el - self.lp.0) * 0.35;
        self.lp.1 += (er - self.lp.1) * 0.35;
        self.l[self.i] = l + (self.lp.0 * 0.8 + self.lp.1 * 0.2) * ECHO_FEEDBACK;
        self.r[self.i] = r + (self.lp.1 * 0.8 + self.lp.0 * 0.2) * ECHO_FEEDBACK;
        self.i = (self.i + 1) % n;
        (el * self.wet, er * self.wet)
    }
}

enum Cmd {
    Speech(crate::recordings::Playback),
    Play(Voice),
    /// Engine loop targets: gain and low-pass cutoff in Hz.
    Thrust(f32, f32),
    Mute(bool),
    /// Music bus target gain (glides with a 0.3 s time constant).
    MusicGain(f32),
    /// Field ambience levels (0 to 1): the energy wall's hum, the
    /// radiation crackle, rock scraping on rock, and the carrier's bay drone.
    Ambience([f32; 4]),
    /// The player's volume sliders (0 to 1): master, effects, music, voice.
    Volumes([f32; 4]),
    /// How much echo the effects get (0 to 1).
    Echo(f32),
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

/// The field's ambient loops. The energy wall hums: two low saws a little
/// apart, so they beat, under a low-pass. Radiation crackles: a Geiger
/// counter's random clicks of high-passed noise, faster the deeper you are.
/// Rock grinding on rock is band-passed noise broken into grains. The
/// carrier's bay drones: two low sines on a slow swell.
#[derive(Default)]
struct Ambience {
    hum: f32,
    hum_to: f32,
    crackle: f32,
    crackle_to: f32,
    scrape: f32,
    scrape_to: f32,
    bay: f32,
    bay_to: f32,
    phase: [f32; 3],
    /// The bay drone's two tones and its swell.
    bay_phase: [f32; 3],
    hum_bq: Biquad,
    click_bq: Biquad,
    scrape_bq: Biquad,
    /// The current click's envelope.
    click: f32,
    /// The scrape's grain envelope: a slow, random wobble.
    grain: f32,
    ready: bool,
}

impl Ambience {
    fn next(&mut self, noise: &mut Noise, sr: f32) -> f32 {
        let dt = 1.0 / sr;
        if !self.ready {
            self.hum_bq.set(Filter::Lowpass, 320.0, sr);
            self.click_bq.set(Filter::Highpass, 2400.0, sr);
            self.scrape_bq.set(Filter::Bandpass, 700.0, sr);
            self.ready = true;
        }
        self.hum += (self.hum_to - self.hum) * (1.0 - (-dt / 0.3).exp());
        self.crackle += (self.crackle_to - self.crackle) * (1.0 - (-dt / 0.15).exp());
        self.scrape += (self.scrape_to - self.scrape) * (1.0 - (-dt / 0.08).exp());
        self.bay += (self.bay_to - self.bay) * (1.0 - (-dt / 0.6).exp());
        let mut out = 0.0;
        if self.hum > 1e-4 {
            let mut x = 0.0;
            for (p, f) in self.phase.iter_mut().zip([55.0, 55.8, 110.0]) {
                *p = (*p + f * dt).fract();
                x += osc(Wave::Sawtooth, *p, f * dt);
            }
            out += self.hum_bq.run(x) * self.hum * 0.09;
        }
        // Clicks per second climb from a few at the rim to a rattle inside.
        let rate = 4.0 + 90.0 * self.crackle;
        if self.crackle > 1e-3 && (noise.next() * 0.5 + 0.5) < rate * dt {
            self.click = 0.5 + 0.5 * (noise.next() * 0.5 + 0.5);
        }
        self.click *= (-dt / 0.003).exp();
        out += self.click_bq.run(noise.next()) * self.click * self.crackle.sqrt() * 0.5;
        let n = noise.next();
        if self.scrape > 1e-4 {
            self.grain += (n.abs() - self.grain) * 0.004;
            out += self.scrape_bq.run(noise.next()) * self.scrape * (0.3 + 1.4 * self.grain) * 0.3;
        }
        if self.bay > 1e-4 {
            for (p, f) in self.bay_phase.iter_mut().zip([41.2, 61.7, 0.25]) {
                *p = (*p + f * dt).fract();
            }
            let swell = 0.7 + 0.3 * (self.bay_phase[2] * TAU).sin();
            let x = (self.bay_phase[0] * TAU).sin() + 0.6 * (self.bay_phase[1] * TAU).sin();
            out += x * swell * self.bay * 0.07;
        }
        out
    }
}

/// Everything the audio callback owns.
struct Mixer {
    sr: f32,
    rx: Receiver<Cmd>,
    voices: Vec<Voice>,
    speech: Option<crate::recordings::Playback>,
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
    ambience: Ambience,
    /// Master, effects, music and voice volume (the sliders).
    vol: [f32; 4],
    /// How far the music is ducked under a radio line (1 = not at all).
    duck: f32,
    echo: Option<Echo>,
}

impl Mixer {
    /// One stereo frame, (left, right).
    fn sample(&mut self) -> (f32, f32) {
        let dt = 1.0 / self.sr;
        // Effects in stereo; the music and the centred loops mono.
        let (mut sl, mut sr) = (0.0, 0.0);
        let mut c = 0.0;
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
                    place,
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
                        let x = place.run(x * env);
                        let (gl, gr) = place.gains();
                        sl += x * gl;
                        sr += x * gr;
                    }
                }
                Voice::Noise {
                    dur,
                    o,
                    t,
                    bq,
                    music,
                    place,
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
                        let x = place.run(x);
                        let (gl, gr) = place.gains();
                        sl += x * gl;
                        sr += x * gr;
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
        c += self.eng_bq.run(self.noise.next()) * self.eng_gain;
        c += self.ambience.next(&mut self.noise, self.sr);
        let [master, sfx, music, voice] = self.vol;
        // Only one-shot effects echo; the steady loops would turn to mush.
        let sample_rate = self.sr;
        let echo = self.echo.get_or_insert_with(|| Echo::new(sample_rate));
        let (el, er) = echo.run(sl, sr, dt);
        sl = (sl + el + c) * sfx;
        sr = (sr + er + c) * sfx;
        self.music_gain += (self.music_gain_to - self.music_gain) * (1.0 - (-dt / 0.3).exp());
        // The music dips under a radio line so the words come through.
        let duck_to = if self.speech.is_some() { DUCK } else { 1.0 };
        let tau = if duck_to < self.duck { 0.08 } else { 0.6 };
        self.duck += (duck_to - self.duck) * (1.0 - (-dt / tau).exp());
        let mut mid = m * self.music_gain * self.duck * music;
        if let Some(speech) = &mut self.speech {
            match speech.sample(self.sr) {
                Some(sample) => mid += sample * voice,
                None => self.speech = None,
            }
        }

        if self.muted {
            return (0.0, 0.0);
        }
        let g = MASTER_VOLUME * master;
        self.compress((sl + mid) * g, (sr + mid) * g)
    }

    /// DynamicsCompressor(threshold -14 dB, knee 10, ratio 4, attack 3 ms,
    /// release 150 ms), as a simple peak-following compressor.
    /// Linked across the two sides: the louder one sets the gain for both,
    /// so the stereo picture doesn't shift as it squashes.
    fn compress(&mut self, l: f32, r: f32) -> (f32, f32) {
        let (thr, knee, ratio) = (-14.0f32, 10.0f32, 4.0f32);
        let lvl = 20.0 * l.abs().max(r.abs()).max(1e-6).log10();
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
        let g = 10f32.powf(-self.comp_db / 20.0);
        ((l * g).clamp(-1.0, 1.0), (r * g).clamp(-1.0, 1.0))
    }

    fn new(sr: f32, rx: Receiver<Cmd>) -> Mixer {
        let mut eng_bq = Biquad::default();
        eng_bq.set(Filter::Lowpass, 400.0, sr);
        Mixer {
            sr,
            rx,
            voices: Vec::new(),
            speech: None,
            noise: Noise(0x1234_5678),
            muted: false,
            eng_gain: 0.0,
            eng_gain_to: 0.0,
            eng_cut: 400.0,
            eng_cut_to: 400.0,
            eng_bq,
            eng_n: 0,
            comp_db: 0.0,
            music_gain: 0.0,
            music_gain_to: 0.0,
            ambience: Ambience::default(),
            vol: [1.0; 4],
            duck: 1.0,
            echo: None,
        }
    }

    fn drain(&mut self) {
        while let Ok(cmd) = self.rx.try_recv() {
            match cmd {
                Cmd::Speech(speech) => self.speech = Some(speech),
                Cmd::Play(mut v) => {
                    let (Voice::Tone { place, .. } | Voice::Noise { place, .. }) = &mut v;
                    if place.muffle > 0.0 {
                        place.bq.set(Filter::Lowpass, place.muffle, self.sr);
                    }
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
                Cmd::Volumes(v) => self.vol = v,
                Cmd::Echo(wet) => {
                    let sr = self.sr;
                    self.echo.get_or_insert_with(|| Echo::new(sr)).wet_to = wet;
                }
                Cmd::Ambience([hum, crackle, scrape, bay]) => {
                    let a = &mut self.ambience;
                    (a.hum_to, a.crackle_to, a.scrape_to, a.bay_to) = (hum, crackle, scrape, bay);
                }
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
    /// Where the next effect is heard from (centred normally; see `sfx_from`).
    place: Place,
    /// The volume sliders, kept to re-send to a reopened device.
    volumes: [f32; 4],
    /// Seconds of game time, for rate-limiting the cargo rattle.
    clock: f64,
    last_rattle: f64,
    rng: Noise,
    /// When the cable last creaked (see `set_field`).
    last_creak: f64,
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
            volumes: [1.0; 4],
            place: Place::default(),
            clock: 0.0,
            last_rattle: -1.0,
            last_creak: -1.0,
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
            // A new mixer starts unmuted at full volume. Music and engine
            // levels are re-sent anyway.
            self.send(Cmd::Mute(self.muted));
            self.send(Cmd::Volumes(self.volumes));
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

    /// Queue recorded speech only while an output stream can receive it.
    pub(crate) fn speech(&self, speech: crate::recordings::Playback) -> bool {
        !self.failed.load(Ordering::Relaxed)
            && self
                .tx
                .as_ref()
                .is_some_and(|tx| tx.send(Cmd::Speech(speech)).is_ok())
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
            place: self.place,
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
            place: Place::default(),
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
            place: Place::default(),
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
            place: Place::default(),
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
            place: self.place,
        }));
    }

    /// The field's sounds for this frame: its four loops, and now and then
    /// a creak from a taut cable (more often the harder it pulls).
    pub fn set_field(&mut self, f: FieldSound) {
        let levels = if self.muted {
            [0.0; 4]
        } else {
            [f.hum, f.crackle, f.scrape, f.bay].map(|x| x.clamp(0.0, 1.0) as f32)
        };
        self.send(Cmd::Ambience(levels));
        let taut = ((f.tension - CREAK_TENSION) / (1.0 - CREAK_TENSION)).clamp(0.0, 1.0) as f32;
        if taut > 0.0
            && self.clock - self.last_creak > CREAK_GAP
            && self.rand(0.0, 1.0) < 0.02 + taut * 0.08
        {
            self.last_creak = self.clock;
            let f0 = self.rand(150.0, 210.0);
            self.tone(
                f0,
                0.35,
                ToneOpts {
                    wave: Wave::Triangle,
                    vol: 0.025 + 0.03 * taut,
                    slide: 0.8,
                    attack: 0.03,
                    vib: 25.0,
                    vib_rate: 11.0,
                    ..TONE
                },
            );
        }
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

    /// Play one effect from a place: `pan` from -1 (left) to 1 (right),
    /// `muffle` a low-pass cutoff in Hz for a distant one (0 = clear), and
    /// `gain` its volume as in `sfx_at`.
    pub fn sfx_from(&mut self, s: Sfx, gain: f32, pan: f32, muffle: f32) {
        self.place = Place {
            pan,
            muffle,
            bq: Biquad::default(),
        };
        self.sfx_at(s, gain);
        self.place = Place::default();
    }

    /// The volume sliders, each 0 to 1: master, effects, music, voice.
    pub fn set_volumes(&mut self, v: [f32; 4]) {
        self.volumes = v.map(|x| x.clamp(0.0, 1.0));
        self.send(Cmd::Volumes(self.volumes));
    }

    /// How much echo the effects get, 0 (none) to 1.
    pub fn set_echo(&self, wet: f32) {
        self.send(Cmd::Echo(wet.clamp(0.0, 1.0)));
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
            // EMP pulse: an electric zap down the scale; a ship caught in it
            // answers with its engine winding down.
            Sfx::Emp(caught) => {
                self.tone(
                    2400.0,
                    0.4,
                    ToneOpts {
                        slide: 0.12,
                        vib: 180.0,
                        vib_rate: 60.0,
                        ..t(Sawtooth, 0.05)
                    },
                );
                self.noise(
                    0.35,
                    NoiseOpts {
                        filter: Filter::Bandpass,
                        sweep_to: 600.0,
                        ..n(0.16, 4000.0)
                    },
                );
                if caught {
                    self.tone(
                        900.0,
                        1.2,
                        ToneOpts {
                            slide: 120.0 / 900.0,
                            delay: 0.12,
                            vib: 8.0,
                            vib_rate: 6.0,
                            attack: 0.05,
                            ..t(Sine, 0.06)
                        },
                    );
                }
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
/// The field ambience for the viewed tug, as (hum, crackle), each 0 to 1.
/// The hum rises over the same stretch the energy wall brightens (from 900
/// px to 120 px off the edge); the crackle starts just outside a radiation
/// cloud and is loudest at its heart.
pub fn field_ambience(w: &cj_sim::World) -> FieldSound {
    let s = &w.me().ship;
    if !s.alive {
        return FieldSound::default();
    }
    let (x, y) = (s.b.x, s.b.y);
    let edge = (x - w.edge_lo)
        .min(w.edge_hi - x)
        .min(y - w.edge_lo)
        .min(w.edge_hi - y);
    let hum = 1.0 - smoothstep(120.0, 900.0, edge);
    let r = cj_sim::consts::RAD_CLOUD_RADIUS;
    let crackle = w
        .clouds
        .iter()
        .map(|c| {
            let d = (c.x - x).hypot(c.y - y);
            (c.t / 3.0).min(1.0) * (1.0 - smoothstep(r * 0.2, r + 120.0, d))
        })
        .fold(0.0, f64::max);
    // Rocks grinding: each touching pair near the tug (and your own cargo
    // dragging on a rock) adds by how fast the two slide past each other.
    let touching = |a: &cj_sim::entities::Body, b: &cj_sim::entities::Body| {
        let d = (a.x - b.x).hypot(a.y - b.y);
        (d <= a.r + b.r + 2.0).then(|| (a.vx - b.vx).hypot(a.vy - b.vy))
    };
    let near: Vec<_> = w
        .rocks
        .iter()
        .filter(|r| !r.dead && (r.b.x - x).hypot(r.b.y - y) < SCRAPE_REACH)
        .map(|r| r.b)
        .collect();
    let mut scrape = 0.0;
    for (i, a) in near.iter().enumerate() {
        let fade = 1.0 - (a.x - x).hypot(a.y - y) / SCRAPE_REACH;
        for b in &near[i + 1..] {
            scrape += touching(a, b).map_or(0.0, |v| (v / 200.0).min(1.0) * fade);
        }
    }
    let mine = &w.me().chain;
    for it in w.items.iter().filter(|it| mine.contains(&it.id)) {
        for r in &near {
            scrape += touching(&it.b, r).map_or(0.0, |v| (v / 200.0).min(1.0));
        }
    }
    let bay = (cj_sim::consts::CARRIER_X - x).hypot(cj_sim::consts::CARRIER_Y - y);
    let tension = w
        .items
        .iter()
        .filter(|it| mine.contains(&it.id))
        .map(|it| it.tension)
        .fold(0.0, f64::max);
    FieldSound {
        hum,
        crackle,
        scrape: scrape.min(1.0),
        bay: 1.0 - smoothstep(300.0, 1400.0, bay),
        tension,
    }
}

/// What the field sounds like at the tug this frame, each 0 to 1 (see
/// `field_ambience` and `Audio::set_field`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FieldSound {
    /// The energy wall's hum.
    pub hum: f64,
    /// Radiation's crackle.
    pub crackle: f64,
    /// Rock grinding on rock, or on your cargo.
    pub scrape: f64,
    /// The carrier's bay drone.
    pub bay: f64,
    /// The tautest link of your cable.
    pub tension: f64,
}

/// Rocks further than this from the tug aren't heard scraping, px.
const SCRAPE_REACH: f64 = 900.0;
/// Cable tension (0..1) above which it starts to creak, and the least time
/// between creaks, in seconds.
const CREAK_TENSION: f64 = 0.6;
const CREAK_GAP: f64 = 0.45;

/// Where an effect is heard from, as (pan, muffle) for `sfx_from`: panned
/// by how far left or right of the listener it is, and muffled more the
/// further away (clear within 500 px). A cannon shot without a place leans
/// toward its barrel.
pub fn place_of(s: Sfx, at: Option<(f32, f32)>, me: (f64, f64)) -> (f32, f32) {
    let Some((x, y)) = at else {
        return match s {
            Sfx::Shoot(barrel) => (f32::from(barrel) * 0.15, 0.0),
            _ => (0.0, 0.0),
        };
    };
    let (dx, dy) = (f64::from(x) - me.0, f64::from(y) - me.1);
    let pan = (dx / 700.0).clamp(-1.0, 1.0) as f32 * 0.85;
    let d = dx.hypot(dy);
    let muffle = if d < 500.0 {
        0.0
    } else {
        // 12 kHz at 500 px down to 1.2 kHz at 2000 px and beyond.
        let k = ((d - 500.0) / 1500.0).min(1.0);
        (12_000.0 * (0.1f64).powf(k)) as f32
    };
    (pan, muffle)
}

/// How much echo the effects get where the viewed tug is: a little in
/// open space, more close to the carrier's and the refinery's hulls.
pub fn field_echo(w: &cj_sim::World) -> f32 {
    let s = &w.me().ship.b;
    let near =
        |x: f64, y: f64, reach: f64| 1.0 - smoothstep(reach * 0.4, reach, (x - s.x).hypot(y - s.y));
    let carrier = near(cj_sim::consts::CARRIER_X, cj_sim::consts::CARRIER_Y, 1100.0);
    let refinery = w
        .refinery
        .as_ref()
        .map_or(0.0, |r| near(r.b.x, r.b.y, 900.0));
    (0.15 + 0.3 * carrier.max(refinery)) as f32
}

fn smoothstep(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

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
    let mixer = Mixer::new(config.sample_rate as f32, rx);
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
    device
        .build_output_stream(
            *config,
            move |data: &mut [T], _| {
                mixer.drain();
                for frame in data.chunks_mut(channels) {
                    let (l, r) = mixer.sample();
                    // Mono devices get both sides; extra channels (surround) the
                    // mix of the two.
                    let (l, r, both) = if channels == 1 {
                        let m = T::from_sample((l + r) * 0.5);
                        (m, m, m)
                    } else {
                        let both = T::from_sample((l + r) * 0.5);
                        (T::from_sample(l), T::from_sample(r), both)
                    };
                    for (k, out) in frame.iter_mut().enumerate() {
                        *out = match k {
                            0 => l,
                            1 => r,
                            _ => both,
                        };
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

    /// Rocks sliding against each other near the tug scrape; apart or far
    /// away they don't. The carrier's bay drones close to it, and the
    /// tautest link of your own cable is what creaks.
    #[test]
    fn rocks_scrape_the_bay_drones_and_the_cable_reports_its_tension() {
        use cj_sim::entities::make_rock;
        let mut w = cj_sim::World::new_shift(cj_sim::rng::Rng::seeded(5), (1280.0, 720.0));
        w.rocks.clear();
        w.clouds.clear();
        let mid = (w.edge_lo + w.edge_hi) / 2.0;
        let pl = w.view_pilot;
        w.pilots[pl].ship.alive = true;
        w.pilots[pl].ship.b.x = mid;
        w.pilots[pl].ship.b.y = mid;
        let mut rng = cj_sim::rng::Rng::seeded(6);
        let mut a = make_rock(&mut rng, 2, mid + 200.0, mid, 60.0, 0.0, false);
        let mut b = make_rock(&mut rng, 2, mid + 200.0, mid, -60.0, 0.0, false);
        b.b.x = a.b.x + a.b.r + b.b.r;
        w.rocks.push(a.clone());
        w.rocks.push(b.clone());
        let f = field_ambience(&w);
        assert!(f.scrape > 0.3, "touching rocks scrape: {}", f.scrape);
        // Pulled apart: silent.
        a.b.x -= 50.0;
        w.rocks[0] = a;
        assert_eq!(field_ambience(&w).scrape, 0.0);
        // Far beyond hearing: silent too.
        w.rocks[0].b.x = b.b.x - b.b.r - w.rocks[0].b.r + 2.0 + 5_000.0;
        w.rocks[1].b.x += 5_000.0;
        assert_eq!(field_ambience(&w).scrape, 0.0);
        // The bay drone near the carrier, none well away from it.
        w.pilots[pl].ship.b.x = cj_sim::consts::CARRIER_X + 2000.0;
        w.pilots[pl].ship.b.y = cj_sim::consts::CARRIER_Y;
        assert!(field_ambience(&w).bay < 0.01);
        w.pilots[pl].ship.b.x = cj_sim::consts::CARRIER_X;
        w.pilots[pl].ship.b.y = cj_sim::consts::CARRIER_Y + 200.0;
        assert!(field_ambience(&w).bay > 0.95);
        // Cable tension comes from your own chain only.
        let mut it = w.new_item(cj_sim::items::ItemType::Ore, mid, mid, 0.0, 0.0);
        it.tension = 0.8;
        let id = it.id;
        w.items.push(it);
        assert_eq!(field_ambience(&w).tension, 0.0);
        w.pilots[pl].chain.push(id);
        assert_eq!(field_ambience(&w).tension, 0.8);
    }

    /// The music dips while a radio line plays and comes back after it.
    #[test]
    fn music_ducks_under_a_radio_line() {
        let (_tx, rx) = channel();
        let sr = 8_000.0;
        let mut m = Mixer::new(sr, rx);
        assert_eq!(m.duck, 1.0);
        // Half a second of (silent) speech at 24 kHz.
        let generation = Arc::new(std::sync::atomic::AtomicU64::new(1));
        m.speech = Some(crate::recordings::Playback::new(
            vec![0.0; 12_000],
            generation,
            1,
        ));
        for _ in 0..(sr * 0.3) as usize {
            m.sample();
        }
        assert!(m.duck < DUCK + 0.02, "ducked: {}", m.duck);
        for _ in 0..(sr * 3.0) as usize {
            m.sample();
        }
        assert!(m.speech.is_none() && m.duck > 0.98, "back: {}", m.duck);
    }

    /// A sound to the right pans right and one to the left pans left; far
    /// ones are muffled, near ones clear; a placeless cannon shot leans
    /// toward its barrel.
    #[test]
    fn placed_sounds_pan_by_side_and_muffle_with_distance() {
        let me = (1000.0, 1000.0);
        let (right, near) = place_of(Sfx::Boom, Some((1300.0, 1000.0)), me);
        let (left, _) = place_of(Sfx::Boom, Some((700.0, 1000.0)), me);
        assert!(
            right > 0.3 && left < -0.3 && near == 0.0,
            "{right} {left} {near}"
        );
        let (_, mid) = place_of(Sfx::Boom, Some((2000.0, 1000.0)), me);
        let (_, far) = place_of(Sfx::Boom, Some((4000.0, 1000.0)), me);
        assert!(mid > far && far >= 1199.0, "{mid} {far}");
        assert_eq!(place_of(Sfx::Boom, None, me), (0.0, 0.0));
        assert!(place_of(Sfx::Shoot(1), None, me).0 > 0.0);
        assert!(place_of(Sfx::Shoot(-1), None, me).0 < 0.0);
    }

    /// Centred is full level both sides; hard left is silent on the right;
    /// the power stays constant across the sweep.
    #[test]
    fn panning_keeps_equal_power() {
        let g = |pan| {
            Place {
                pan,
                ..Place::default()
            }
            .gains()
        };
        let (l, r) = g(0.0);
        assert!((l - 1.0).abs() < 1e-5 && (r - 1.0).abs() < 1e-5);
        let (l, r) = g(-1.0);
        assert!(r.abs() < 1e-5 && (l * l - 2.0).abs() < 1e-4);
        for p in [-0.7, -0.2, 0.4, 0.9] {
            let (l, r) = g(p);
            assert!((l * l + r * r - 2.0).abs() < 1e-4, "{p}");
        }
    }

    /// A click comes back from the echo after each side's delay, darker and
    /// quieter, and the echo stays silent when its level is zero.
    #[test]
    fn the_echo_repeats_a_click_on_each_side() {
        let sr = 8_000.0;
        let mut e = Echo::new(sr);
        e.wet = 1.0;
        e.wet_to = 1.0;
        let out: Vec<(f32, f32)> = (0..8_000)
            .map(|i| e.run(if i == 0 { 1.0 } else { 0.0 }, 0.0, 1.0 / sr))
            .collect();
        let first = |side: fn(&(f32, f32)) -> f32| out.iter().position(|o| side(o).abs() > 1e-3);
        assert_eq!(first(|o| o.0), Some((sr * ECHO_L) as usize));
        assert!(first(|o| o.1).unwrap() > (sr * ECHO_L) as usize);
        assert!(out.iter().all(|o| o.0.abs() <= 1.0 && o.1.abs() <= 1.0));
        let mut quiet = Echo::new(sr);
        assert!((0..2_000)
            .all(|i| quiet.run(if i == 0 { 1.0 } else { 0.0 }, 0.0, 1.0 / sr) == (0.0, 0.0)));
    }

    /// The ambient loops are silent at zero and stay well inside full
    /// scale at full level.
    #[test]
    fn ambience_is_silent_off_and_bounded_on() {
        let mut noise = Noise(0x1234_5678);
        let mut a = Ambience::default();
        assert!((0..4800).all(|_| a.next(&mut noise, 48_000.0) == 0.0));
        a.hum_to = 1.0;
        a.crackle_to = 1.0;
        let peak = (0..96_000)
            .map(|_| a.next(&mut noise, 48_000.0).abs())
            .fold(0.0, f32::max);
        assert!(peak > 0.01 && peak < 0.9, "{peak}");
    }

    /// The wall hums louder as the tug nears it, and radiation crackles
    /// only in and around a cloud.
    #[test]
    fn the_wall_hums_and_radiation_crackles_near_the_tug() {
        let mut w = cj_sim::World::new_shift(cj_sim::rng::Rng::seeded(4), (1280.0, 720.0));
        w.clouds.clear();
        let mid = (w.edge_lo + w.edge_hi) / 2.0;
        // Clear of rocks and the carrier, so only the wall and radiation count.
        w.rocks.clear();
        let at = |w: &mut cj_sim::World, x: f64| {
            w.pilots[w.view_pilot].ship.alive = true;
            w.pilots[w.view_pilot].ship.b.x = x;
            w.pilots[w.view_pilot].ship.b.y = mid;
            let f = field_ambience(w);
            (f.hum, f.crackle)
        };
        assert_eq!(at(&mut w, mid), (0.0, 0.0));
        let hi = w.edge_hi;
        let far = at(&mut w, hi - 700.0).0;
        let near = at(&mut w, hi - 200.0).0;
        assert!(0.0 < far && far < near && near < 1.0, "{far} {near}");
        w.clouds.push(cj_sim::entities::RadCloud {
            x: mid,
            y: mid,
            t: 20.0,
        });
        assert_eq!(at(&mut w, mid).1, 1.0);
        let r = cj_sim::consts::RAD_CLOUD_RADIUS;
        assert!(at(&mut w, mid + r).1 > 0.0);
        assert_eq!(at(&mut w, mid + r + 200.0).1, 0.0);
    }

    #[test]
    fn recorded_dialogue_drops_routine_backlog_and_obeys_interrupt_and_mute() {
        let (tx, rx) = channel();
        let mut audio = Audio {
            tx: Some(tx),
            _stream: None,
            failed: Arc::new(AtomicBool::new(false)),
            reopen_at: 0.0,
            muted: false,
            sfx_scale: 1.0,
            volumes: [1.0; 4],
            place: Place::default(),
            clock: 0.0,
            last_rattle: -1.0,
            last_creak: -1.0,
            rng: Noise(1),
            error: None,
        };
        let mut voice = crate::marge::Voice::new(true);
        voice.say("Banked.", false, None, None, &mut audio);
        let Cmd::Speech(mut first) = rx.try_recv().expect("first line queued") else {
            panic!("speech expected")
        };
        voice.say("Banked.", false, None, None, &mut audio);
        assert!(
            rx.try_recv().is_err(),
            "routine speech must not queue a backlog"
        );
        assert!(first.sample(48_000.0).is_some());
        voice.say("Banked.", true, None, None, &mut audio);
        assert_eq!(
            first.sample(48_000.0),
            None,
            "urgent speech cuts the old line"
        );
        let Cmd::Speech(mut urgent) = rx.try_recv().expect("urgent line queued") else {
            panic!("speech expected")
        };
        voice.toggle();
        assert_eq!(urgent.sample(48_000.0), None, "voice toggle stops playback");
        voice.say("Banked.", true, None, None, &mut audio);
        assert!(rx.try_recv().is_err());
        voice.toggle();
        audio.muted = true;
        voice.say("Banked.", true, None, None, &mut audio);
        assert!(rx.try_recv().is_err(), "muted speech must not queue");
    }

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
            volumes: [1.0; 4],
            place: Place::default(),
            clock: 0.0,
            last_rattle: -1.0,
            last_creak: -1.0,
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
        let mut m = Mixer::new(48_000.0, rx);
        let mut last = 0.0;
        for i in 0..48_000 {
            let x = (i as f32 * 440.0 / 48_000.0 * TAU).sin() * 3.0;
            let (l, r) = m.compress(x, x * 0.5);
            last = l.abs().max(r.abs()).max(last * 0.999);
        }
        assert!(last <= 1.0);
        assert!(m.comp_db > 6.0, "it is reducing gain: {} dB", m.comp_db);
    }
}
