//! Marge on the dispatch radio (src/js/04-dispatcher-radio.js and the
//! radio() function in 11-guided-first-shift.js): the radio panel in the
//! bottom-left corner, its throttle, her bundled neural voice recordings,
//! and the guided-training card.

use crate::audio::Audio;
use crate::lines::{callout, lines, ASIDES};
use crate::look::{col, rgba, text_left, text_width, Look};
use crate::ui::{wrap, Which};
use cj_sim::events::Radio;
use cj_sim::rng::Rng;
use cj_sim::tutorial::TUTORIAL;
use cj_sim::World;
use macroquad::prelude::*;

/// Routine lines wait this long after the last one (seconds of shift time).
const RADIO_GAP: f64 = 3.5;
/// A line stays bright this long, then the panel fades to quiet.
const RADIO_HOLD: f64 = 5.5;

/// Bundled neural voices; the radio text remains available without sound.
pub struct Voice {
    clips: Vec<Vec<f32>>,
    generation: std::sync::Arc<std::sync::atomic::AtomicU64>,
    until: std::time::Instant,
    pub enabled: bool,
    pub status: String,
}

impl Voice {
    pub fn new(enabled: bool) -> Voice {
        let clips: Option<Vec<_>> = crate::recordings::ASSETS
            .iter()
            .map(|a| crate::recordings::decode(a.wav))
            .collect();
        let status = match &clips {
            Some(c) => format!(
                "{} bundled neural recordings; Marge: bf_emma; cockpit: af_heart",
                c.len()
            ),
            None => "unavailable (invalid bundled voice WAV)".to_owned(),
        };
        Voice {
            clips: clips.unwrap_or_default(),
            generation: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            until: std::time::Instant::now(),
            enabled,
            status,
        }
    }

    /// Urgent lines interrupt; routine lines never build a speech backlog.
    pub fn say(
        &mut self,
        text: &str,
        urgent: bool,
        callout: Option<&str>,
        aside: Option<&str>,
        audio: &mut Audio,
    ) {
        if !self.enabled || audio.is_muted() || self.clips.is_empty() {
            return;
        }
        if !urgent && std::time::Instant::now() < self.until {
            return;
        }
        self.stop();
        let mut samples = Vec::new();
        for (line, system) in callout
            .map(|s| (s, true))
            .into_iter()
            .chain(aside.map(|s| (s, false)))
            .chain(std::iter::once((text, false)))
        {
            let Some(plan) = crate::recordings::plan(line, system) else {
                eprintln!("recording missing for radio line: {line}");
                return;
            };
            for i in plan {
                samples.extend_from_slice(&self.clips[i]);
            }
            samples.resize(samples.len() + 2_400, 0.0);
        }
        if callout.is_some() {
            audio.caution();
        }
        let duration = samples.len() as f64 / 24_000.0;
        let tag = self.generation.load(std::sync::atomic::Ordering::Relaxed);
        let speech = crate::recordings::Playback::new(samples, self.generation.clone(), tag);
        if audio.speech(speech) {
            self.until = std::time::Instant::now() + std::time::Duration::from_secs_f64(duration);
        }
    }

    pub fn stop(&mut self) {
        self.generation
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.until = std::time::Instant::now();
    }

    pub fn toggle(&mut self) {
        self.enabled = !self.enabled;
        if !self.enabled {
            self.stop();
        }
    }
}

impl Drop for Voice {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The radio panel's state for this shift (radioState).
pub struct RadioPanel {
    last_at: f64,
    until: f64,
    used: Vec<Radio>,
    pub text: String,
    /// Lines shown since the game started (for the smoke report).
    lines_shown: u32,
}

impl Default for RadioPanel {
    fn default() -> Self {
        RadioPanel {
            last_at: -99.0,
            until: 0.0,
            used: Vec::new(),
            text: "Channel open.".to_owned(),
            lines_shown: 0,
        }
    }
}

impl RadioPanel {
    /// A new shift: the throttle and the once-per-shift lines reset.
    pub fn reset(&mut self) {
        self.last_at = -99.0;
        self.used.clear();
    }

    pub fn lines_shown(&self) -> u32 {
        self.lines_shown
    }

    /// radio(): throttle, pick the line, show it, blip, and say it.
    #[allow(clippy::too_many_arguments)]
    pub fn line(
        &mut self,
        key: Radio,
        force: bool,
        once: bool,
        text: Option<&str>,
        now: f64,
        rng: &mut Rng,
        audio: &mut Audio,
        voice: &mut Voice,
    ) {
        if once && self.used.contains(&key) {
            return;
        }
        if !force && now - self.last_at < RADIO_GAP {
            return;
        }
        let shown = match text {
            Some(t) => t.to_owned(),
            None => {
                let pool = lines(key);
                if pool.is_empty() {
                    return;
                }
                pool[rng.index(pool.len())].to_owned()
            }
        };
        if !self.used.contains(&key) {
            self.used.push(key);
        }
        self.last_at = now;
        self.until = now + RADIO_HOLD;
        // About a third of routine lines open with one of Marge's sighs;
        // never on urgent or warning lines.
        let co = callout(key);
        let aside =
            (!force && co.is_none() && rng.next() < 0.33).then(|| ASIDES[rng.index(ASIDES.len())]);
        self.text = match aside {
            Some(a) => format!("{a} {shown}"),
            None => shown.clone(),
        };
        self.lines_shown += 1;
        audio.radio_blip();
        // The browser reads the full line aloud (SPEAK_SHORT_LINES is false).
        voice.say(&shown, force, co, aside, audio);
    }

    /// The panel: bottom-left, a hazard edge, fading to quiet between lines.
    pub fn draw(&self, look: &Look, now: f64, view: (f32, f32)) {
        let (vw, vh) = view;
        let max_w = 420.0f32.min(vw - 32.0);
        let lines = wrap(look, &self.text, 16.0, Which::Body, max_w - 24.0 - 3.0);
        let lh = 16.0 * 1.3;
        let h = 8.0 + 14.0 + lines.len() as f32 * lh + 8.0;
        let w = lines
            .iter()
            .map(|l| text_width(l, 16.0, &look.fonts.body))
            .fold(
                text_width("DISPATCH · MARGE", 10.0, &look.fonts.mono) * 1.2,
                f32::max,
            )
            + 24.0
            + 3.0;
        let (x, y) = (16.0, vh - 16.0 - h);
        // .quiet fades the panel to 25% over 400 ms after the hold.
        let q = ((now - self.until) / 0.4).clamp(0.0, 1.0) as f32;
        let a = 1.0 - 0.75 * q;
        draw_rectangle(
            x,
            y,
            w,
            h,
            Color::new(9.0 / 255.0, 13.0 / 255.0, 21.0 / 255.0, 0.86 * a),
        );
        draw_rectangle(x, y, 3.0, h, rgba(col::HAZARD, a));
        let mut cx = x + 15.0;
        for ch in "DISPATCH · MARGE".chars() {
            let s = ch.to_string();
            cx += text_left(
                &s,
                cx,
                y + 8.0,
                10.0,
                &look.fonts.mono,
                rgba(col::HAZARD, a),
            ) + 1.6;
        }
        let mut ly = y + 22.0;
        for l in &lines {
            text_left(l, x + 15.0, ly, 16.0, &look.fonts.body, rgba(col::TEXT, a));
            ly += lh;
        }
    }
}

/// The training card (#hud-tut): bottom centre, the step and Marge's
/// instruction, with a skip button. Returns true when skip is clicked.
pub fn draw_tutorial(look: &Look, w: &World, view: (f32, f32)) -> bool {
    let Some(t) = &w.tut else { return false };
    let (vw, vh) = view;
    let cw = 440.0f32.min(vw - 32.0);
    let text = TUTORIAL[t.step.min(TUTORIAL.len() - 1)];
    let lines = wrap(look, text, 15.0, Which::Body, cw - 28.0);
    let lh = 15.0 * 1.4;
    let btn = "Skip training (T)";
    let bw = text_width(btn, 12.0, &look.fonts.mono) + 20.0;
    let bh = 24.0;
    let h = 10.0 + 14.0 + 4.0 + lines.len() as f32 * lh + 8.0 + bh + 10.0;
    let (x, y) = ((vw - cw) / 2.0, vh - 20.0 - h);
    draw_rectangle(
        x,
        y,
        cw,
        h,
        Color::new(10.0 / 255.0, 14.0 / 255.0, 24.0 / 255.0, 0.88),
    );
    draw_rectangle_lines(x, y, cw, h, 1.0, crate::look::hex(col::HAZARD));
    let label = format!("TRAINING · {} / {}", t.step + 1, TUTORIAL.len());
    let mut cx = x + 14.0;
    for ch in label.chars() {
        let s = ch.to_string();
        cx += text_left(
            &s,
            cx,
            y + 10.0,
            11.0,
            &look.fonts.mono_light,
            crate::look::hex(col::MUTED),
        ) + 1.5;
    }
    let mut ly = y + 28.0;
    for l in &lines {
        text_left(
            l,
            x + 14.0,
            ly,
            15.0,
            &look.fonts.body,
            crate::look::hex(col::TEXT),
        );
        ly += lh;
    }
    let br = Rect::new(x + 14.0, ly + 8.0, bw, bh);
    // The HUD may be drawn magnified; the mouse is in window units.
    let hover = br.contains(Vec2::from(mouse_position()) / crate::look::hud_scale());
    draw_rectangle_lines(
        br.x,
        br.y,
        br.w,
        br.h,
        1.0,
        crate::look::hex(if hover { col::TEXT } else { col::PANEL_EDGE }),
    );
    text_left(
        btn,
        br.x + 10.0,
        br.y + 5.0,
        12.0,
        &look.fonts.mono,
        crate::look::hex(col::TEXT),
    );
    hover && is_mouse_button_pressed(MouseButton::Left)
}
