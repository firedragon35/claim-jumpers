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
use std::sync::mpsc::{channel, Receiver, Sender};

/// Master volume and the per-effect gain (MASTER_VOLUME, SFX_GAIN).
const MASTER_VOLUME: f32 = 0.95;
const SFX_GAIN: f32 = 1.9;
/// Envelopes start and end here rather than at 0, because an exponential
/// ramp cannot reach zero (WebAudio's 0.0001 floor).
const FLOOR: f32 = 0.0001;

#[derive(Clone, Copy, PartialEq)]
enum Wave {
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

/// A sounding voice, in seconds since it was queued.
enum Voice {
    Tone {
        freq: f32,
        dur: f32,
        o: ToneOpts,
        t: f32,
        phase: f32,
        lfo: f32,
    },
    Noise {
        dur: f32,
        o: NoiseOpts,
        t: f32,
        bq: Biquad,
    },
}

enum Cmd {
    Play(Voice),
    /// Engine loop targets: gain and low-pass cutoff in Hz.
    Thrust(f32, f32),
    Mute(bool),
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
}

impl Mixer {
    fn sample(&mut self) -> f32 {
        let dt = 1.0 / self.sr;
        let mut s = 0.0;
        for v in &mut self.voices {
            match v {
                Voice::Tone {
                    freq,
                    dur,
                    o,
                    t,
                    phase,
                    lfo,
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
                    let vol = (o.vol * SFX_GAIN).min(1.0);
                    let env = if lt < att {
                        exp_ramp(FLOOR, vol, lt / att)
                    } else {
                        exp_ramp(vol, FLOOR, (lt - att) / (*dur - att))
                    };
                    s += osc(o.wave, *phase, inc) * env;
                }
                Voice::Noise { dur, o, t, bq } => {
                    let lt = *t - o.delay;
                    *t += dt;
                    if lt < 0.0 {
                        continue;
                    }
                    let k = lt / *dur;
                    if o.sweep_to != 0.0 {
                        bq.set(o.filter, exp_ramp(o.freq, o.sweep_to, k), self.sr);
                    }
                    let vol = (o.vol * SFX_GAIN).min(1.0);
                    s += bq.run(self.noise.next()) * exp_ramp(vol, FLOOR, k);
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
        if self.eng_n % 32 == 0 {
            self.eng_bq
                .set(Filter::Lowpass, self.eng_cut.max(20.0), self.sr);
        }
        s += self.eng_bq.run(self.noise.next()) * self.eng_gain;

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
                    if let Voice::Noise { o, bq, .. } = &mut v {
                        bq.set(o.filter, o.freq, self.sr);
                    }
                    self.voices.push(v);
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
    muted: bool,
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
        match open() {
            Ok((tx, stream)) => Audio {
                tx: Some(tx),
                _stream: Some(stream),
                muted: false,
                clock: 0.0,
                last_rattle: -1.0,
                rng: Noise(0x9e37_79b9),
                error: None,
            },
            Err(e) => Audio {
                tx: None,
                _stream: None,
                muted: false,
                clock: 0.0,
                last_rattle: -1.0,
                rng: Noise(0x9e37_79b9),
                error: Some(e),
            },
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

    fn send(&self, c: Cmd) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(c);
        }
    }

    /// `rand(a, b)` for pitch variation.
    fn rand(&mut self, a: f32, b: f32) -> f32 {
        a + (self.rng.next() * 0.5 + 0.5) * (b - a)
    }

    fn tone(&self, freq: f32, dur: f32, o: ToneOpts) {
        if self.muted {
            return;
        }
        self.send(Cmd::Play(Voice::Tone {
            freq,
            dur,
            o,
            t: 0.0,
            phase: 0.0,
            lfo: 0.0,
        }));
    }

    fn noise(&self, dur: f32, o: NoiseOpts) {
        if self.muted {
            return;
        }
        self.send(Cmd::Play(Voice::Noise {
            dur,
            o,
            t: 0.0,
            bq: Biquad::default(),
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
                if d > 1100.0 {
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

fn open() -> Result<(Sender<Cmd>, cpal::Stream), String> {
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
    };
    let stream = match format {
        cpal::SampleFormat::F32 => build::<f32>(&device, &config, mixer),
        cpal::SampleFormat::I16 => build::<i16>(&device, &config, mixer),
        cpal::SampleFormat::U16 => build::<u16>(&device, &config, mixer),
        other => return Err(format!("unsupported sample format {other:?}")),
    }?;
    stream.play().map_err(|e| e.to_string())?;
    Ok((tx, stream))
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut mixer: Mixer,
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
            |err| eprintln!("audio stream error: {err}"),
            None,
        )
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

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
