//! Marge on the dispatch radio (src/js/04-dispatcher-radio.js and the
//! radio() function in 11-guided-first-shift.js): the radio panel in the
//! bottom-left corner, its throttle, her spoken voice through the Windows
//! speech synthesizer, and the guided-training card.

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

/* Speech delivery (pitch and rate, 1 = normal). Marge is low, unhurried
 * and dry; her last sentence (usually the jab) lands lower and slower. The
 * warning system is crisp and flat, so the two read as different speakers. */
const MARGE_VOICE: (f32, f32) = (0.62, 0.88);
const MARGE_PUNCHLINE: (f32, f32) = (0.56, 0.82);
const MARGE_ASIDE: (f32, f32) = (0.54, 0.78);
const SYSTEM_VOICE: (f32, f32) = (1.05, 1.0);

/// Marge's voice. Unavailable speech leaves the text radio working.
pub struct Voice {
    tts: Option<tts::Tts>,
    marge: Option<tts::Voice>,
    system: Option<tts::Voice>,
    pub enabled: bool,
    /// What happened when speech started, for the smoke report.
    pub status: String,
}

impl Voice {
    pub fn new(enabled: bool) -> Voice {
        let mut tts = match tts::Tts::default() {
            Ok(t) => t,
            Err(e) => {
                return Voice {
                    tts: None,
                    marge: None,
                    system: None,
                    enabled,
                    status: format!("unavailable ({e})"),
                }
            }
        };
        let voices: Vec<tts::Voice> = tts
            .voices()
            .unwrap_or_default()
            .into_iter()
            .filter(|v| v.language().as_str().to_lowercase().starts_with("en"))
            .collect();
        let find = |names: &[&str]| {
            voices
                .iter()
                .find(|v| {
                    let n = v.name().to_lowercase();
                    names.iter().any(|w| n.contains(w))
                })
                .cloned()
        };
        // Marge: the mellower UK/AU-style female voices first, then any
        // female voice; the warning system prefers a crisp US voice.
        let marge = find(&[
            "hazel", "susan", "moira", "karen", "libby", "sonia", "serena", "tessa", "victoria",
        ])
        .or_else(|| find(&["zira", "samantha", "aria", "jenny", "natasha", "female"]))
        .or_else(|| voices.first().cloned());
        let system = find(&["zira", "aria", "jenny", "samantha"]).or_else(|| marge.clone());
        let _ = tts.set_volume(1.0);
        let status = format!(
            "{} English voice(s); Marge: {}",
            voices.len(),
            marge
                .as_ref()
                .map_or("system default".to_owned(), |v| v.name())
        );
        Voice {
            tts: Some(tts),
            marge,
            system,
            enabled,
            status,
        }
    }

    fn speak(&mut self, text: &str, (pitch, rate): (f32, f32), system: bool) {
        let voice = if system { &self.system } else { &self.marge };
        let Some(t) = self.tts.as_mut() else { return };
        if let Some(v) = voice {
            let _ = t.set_voice(v);
        }
        let _ = t.set_pitch(pitch);
        let _ = t.set_rate(rate);
        let _ = t.speak(text, false);
    }

    /// Speak a radio line. Urgent lines cut off whatever she is saying;
    /// routine ones are dropped while she is mid-sentence, so the radio
    /// never queues a backlog.
    pub fn say(
        &mut self,
        text: &str,
        urgent: bool,
        callout: Option<&str>,
        aside: Option<&str>,
        audio: &mut Audio,
    ) {
        if !self.enabled || audio.is_muted() {
            return;
        }
        let Some(t) = self.tts.as_mut() else { return };
        if t.is_speaking().unwrap_or(false) {
            if !urgent {
                return;
            }
            let _ = t.stop();
        }
        if let Some(c) = callout {
            audio.caution();
            self.speak(c, SYSTEM_VOICE, true);
        }
        if let Some(a) = aside {
            self.speak(a, MARGE_ASIDE, false);
        }
        // Speech reads straight quotes awkwardly and says "cr" as letters.
        let spoken = spoken_text(text);
        let sentences = sentences(&spoken);
        let n = sentences.len();
        for (i, s) in sentences.iter().enumerate() {
            let last = n > 1 && i == n - 1;
            self.speak(s, if last { MARGE_PUNCHLINE } else { MARGE_VOICE }, false);
        }
    }

    pub fn stop(&mut self) {
        if let Some(t) = self.tts.as_mut() {
            let _ = t.stop();
        }
    }

    pub fn toggle(&mut self) {
        self.enabled = !self.enabled;
        if !self.enabled {
            self.stop();
        }
    }
}

/// Text tidied for the ear: no quote marks, and "1,200 cr" read as credits.
fn spoken_text(text: &str) -> String {
    let t: String = text
        .chars()
        .filter(|c| !matches!(c, '"' | '\u{201c}' | '\u{201d}'))
        .collect();
    let mut out = String::with_capacity(t.len());
    let b: Vec<char> = t.chars().collect();
    let mut i = 0;
    while i < b.len() {
        // (\d)\s*cr\b -> "$1 credits"
        if b[i].is_ascii_digit() {
            let mut j = i + 1;
            while j < b.len() && b[j] == ' ' {
                j += 1;
            }
            let word_end = j + 2 >= b.len() || !b[j + 2].is_alphanumeric();
            if j + 1 < b.len() && b[j] == 'c' && b[j + 1] == 'r' && word_end {
                out.push(b[i]);
                out.push_str(" credits");
                i = j + 2;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    out
}

/// Split into sentences the way /[^.!?]+[.!?]*\s*/g does, trimmed.
fn sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut ended = false;
    for ch in text.chars() {
        let stop = matches!(ch, '.' | '!' | '?');
        if ended && !stop && !ch.is_whitespace() {
            out.push(cur.trim().to_owned());
            cur.clear();
            ended = false;
        }
        cur.push(ch);
        if stop {
            ended = true;
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_owned());
    }
    out
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
    let hover = br.contains(mouse_position().into());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speech_reads_credits_and_drops_quotes() {
        assert_eq!(
            spoken_text("Bounty of 1,400 cr on \"them\"."),
            "Bounty of 1,400 credits on them."
        );
        assert_eq!(spoken_text("400cr crate"), "400 credits crate");
        // "cr" inside a word stays.
        assert_eq!(spoken_text("3 crews"), "3 crews");
    }

    #[test]
    fn lines_split_into_sentences_like_the_browser() {
        assert_eq!(
            sentences("Ion storm. Shields down! Fly... like you mean it"),
            vec!["Ion storm.", "Shields down!", "Fly...", "like you mean it"]
        );
        assert_eq!(sentences("Banked."), vec!["Banked."]);
    }
}
