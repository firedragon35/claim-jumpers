//! What only an online shift draws: each pilot's tug colour, and the
//! player list held up with Tab.

use crate::look::{col, hex, rgba, text_left, text_width, Look};
use macroquad::prelude::*;
use std::collections::VecDeque;

/// The trim colour of the tug in each pilot slot, chosen so neighbouring
/// slots differ clearly. Every client derives a pilot's colour from its
/// slot, so all players see the same colours without the server sending
/// them. (On your own screen, your tug keeps your paint job.)
const SLOT_TRIMS: [u32; cj_server::MAX_PLAYERS] = [
    0xff5a5a, 0x4fc3ff, 0x7dff6b, 0xffb83d, 0xc77dff, 0x3dffd2, 0xff7de0, 0xfff36b, 0x7d8cff,
    0xff8f4f, 0x9bff3d, 0x3d9bff, 0xff3d8f, 0x3dff8f, 0xd2a36b, 0xb0b8c8, 0x8f3dff, 0x6bffff,
    0xffd2a3, 0xa3ff6b,
];

/// Scales each colour channel of `rgb` by `k` (0 to 1).
fn darken(rgb: u32, k: f32) -> u32 {
    let ch = |shift: u32| ((((rgb >> shift) & 0xff) as f32 * k).round() as u32) << shift;
    ch(16) | ch(8) | ch(0)
}

/// The (hull, trim) paint of the tug in pilot slot `slot`: a bright trim
/// over a hull of the same hue, darkened.
pub fn slot_paint(slot: usize) -> (u32, u32) {
    let trim = SLOT_TRIMS[slot % SLOT_TRIMS.len()];
    (darken(trim, 0.16), trim)
}

/// The list held up with Tab: every player on the server with this
/// shift's banked credits, kills and deaths, best first, in their tug's
/// colour. It sits at the top of the screen over the field, which stays in
/// view around it.
pub fn player_list(
    look: &Look,
    lobby: Option<&cj_net::Lobby>,
    you: Option<usize>,
    view: (f32, f32),
) {
    let Some(l) = lobby else {
        return;
    };
    let mut rows: Vec<&cj_net::LobbyPlayer> = l.players.iter().collect();
    rows.sort_by_key(|p| std::cmp::Reverse(p.banked));
    let (w, line) = (520.0_f32.min(view.0 - 32.0), 22.0);
    let h = 56.0 + line * rows.len() as f32;
    let (x, y) = ((view.0 - w) / 2.0, 72.0);
    draw_rectangle(
        x,
        y,
        w,
        h,
        Color::new(9.0 / 255.0, 13.0 / 255.0, 21.0 / 255.0, 0.82),
    );
    draw_rectangle_lines(x, y, w, h, 1.0, hex(col::PANEL_EDGE));
    let font = &look.fonts.mono;
    let cols = [x + 34.0, x + w - 230.0, x + w - 130.0, x + w - 60.0];
    let head = rgba(col::TEXT, 0.55);
    text_left(
        &format!("PILOTS {}/{}", l.players.len(), l.max),
        cols[0],
        y + 24.0,
        12.0,
        font,
        head,
    );
    for (i, s) in ["BANKED", "KILLS", "DEATHS"].iter().enumerate() {
        text_left(s, cols[i + 1], y + 24.0, 12.0, font, head);
    }
    for (i, p) in rows.iter().enumerate() {
        let ry = y + 50.0 + line * i as f32;
        let mine = p.slot.map(usize::from) == you && you.is_some();
        let colour = if p.away || p.slot.is_none() {
            rgba(col::TEXT, 0.45)
        } else if mine {
            hex(col::HAZARD)
        } else {
            hex(col::TEXT)
        };
        if let Some(s) = p.slot {
            draw_rectangle(
                x + 14.0,
                ry - 10.0,
                10.0,
                10.0,
                hex(slot_paint(usize::from(s)).1),
            );
        }
        let tag = if p.slot.is_none() {
            " (next shift)"
        } else if p.away {
            " (away)"
        } else {
            ""
        };
        text_left(&format!("{}{tag}", p.name), cols[0], ry, 13.0, font, colour);
        let cells = [
            cj_sim::world::fmt_credits(p.banked as f64),
            p.kills.to_string(),
            p.deaths.to_string(),
        ];
        for (j, s) in cells.iter().enumerate() {
            text_left(s, cols[j + 1], ry, 13.0, font, colour);
        }
    }
}

/// Chat lines kept for the screen.
const CHAT_KEEP: usize = 6;
/// Seconds a chat line stays up, then the seconds it takes to fade.
const CHAT_HOLD: f64 = 10.0;
const CHAT_FADE: f64 = 2.0;
/// Room left under the chat for Marge's radio panel, which shares the
/// bottom-left corner.
const CHAT_ABOVE_RADIO: f32 = 120.0;

/// The public chat as this client has seen it: each line with the moment
/// it arrived (in `get_time` seconds).
#[derive(Default)]
pub struct Chat {
    lines: VecDeque<(f64, cj_net::ChatLine)>,
    /// The line being typed, while the chat is open.
    pub typing: Option<String>,
}

impl Chat {
    /// Keeps `line`, received at `now`; only the newest few are kept.
    pub fn push(&mut self, line: cj_net::ChatLine, now: f64) {
        self.lines.push_back((now, line));
        while self.lines.len() > CHAT_KEEP {
            self.lines.pop_front();
        }
    }

    /// Forgets everything (on leaving a server).
    pub fn clear(&mut self) {
        self.lines.clear();
        self.typing = None;
    }

    /// The chat in the bottom-left corner, over the field: each line in
    /// its sender's tug colour, fading out after a while unless the chat
    /// is open, and the line being typed under them. Only the text has a
    /// shadow, so the field shows through.
    pub fn draw(&self, look: &Look, now: f64, view: (f32, f32)) {
        let font = &look.fonts.mono;
        let (size, lh) = (13.0, 18.0);
        let max_w = (view.0 * 0.45).clamp(240.0, 560.0);
        let mut y = view.1 - CHAT_ABOVE_RADIO;
        if let Some(t) = &self.typing {
            let line = format!("SAY: {t}_");
            let w = text_width(&line, size, font) + 16.0;
            draw_rectangle(12.0, y - 14.0, w, lh + 2.0, Color::new(0.0, 0.0, 0.0, 0.55));
            text_left(&line, 20.0, y, size, font, hex(col::HAZARD));
            y -= lh + 6.0;
        }
        // Newest at the bottom: lay the lines out from the bottom up.
        let mut rows: Vec<(f32, Color, Color, String, String)> = Vec::new();
        for (at, l) in self.lines.iter().rev() {
            let age = now - at;
            let a = if self.typing.is_some() {
                1.0
            } else {
                (1.0 - (age - CHAT_HOLD) / CHAT_FADE).clamp(0.0, 1.0) as f32
            };
            if a <= 0.0 {
                continue;
            }
            let who = match l.slot {
                Some(s) => rgba(slot_paint(usize::from(s)).1, a),
                None => rgba(col::TEXT, 0.6 * a),
            };
            let name = format!("{}: ", l.from);
            for (i, part) in wrap(&l.text, max_w - text_width(&name, size, font), |t| {
                text_width(t, size, font)
            })
            .into_iter()
            .enumerate()
            .rev()
            {
                let lead = if i == 0 { name.clone() } else { String::new() };
                rows.push((a, who, rgba(col::TEXT, a), lead, part));
            }
        }
        for (a, who, body, lead, part) in rows {
            let shadow = Color::new(0.0, 0.0, 0.0, 0.8 * a);
            let x = 20.0;
            let lw = text_width(&lead, size, font);
            text_left(&lead, x + 1.0, y + 1.0, size, font, shadow);
            text_left(&lead, x, y, size, font, who);
            text_left(&part, x + lw + 1.0, y + 1.0, size, font, shadow);
            text_left(&part, x + lw, y, size, font, body);
            y -= lh;
        }
    }
}

/// Splits `text` at spaces into lines no wider than `max_w` by `width`; a
/// single word wider than that gets a line of its own.
pub fn wrap(text: &str, max_w: f32, width: impl Fn(&str) -> f32) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    for word in text.split(' ') {
        let next = if cur.is_empty() {
            word.to_owned()
        } else {
            format!("{cur} {word}")
        };
        if width(&next) > max_w && !cur.is_empty() {
            lines.push(std::mem::take(&mut cur));
            cur = word.to_owned();
        } else {
            cur = next;
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every slot a server can hold has its own colour, with a dark hull.
    #[test]
    fn each_slot_has_its_own_colour() {
        let trims: std::collections::HashSet<u32> = (0..cj_server::MAX_PLAYERS)
            .map(|s| slot_paint(s).1)
            .collect();
        assert_eq!(trims.len(), cj_server::MAX_PLAYERS);
        let (hull, trim) = slot_paint(0);
        assert!(hull < trim && hull & 0xff0000 <= 0x2a0000);
    }

    /// Chat text wraps at spaces to the width, one long word to a line.
    #[test]
    fn chat_text_wraps_at_spaces() {
        let w = |t: &str| t.len() as f32;
        assert_eq!(wrap("aa bb cc", 5.0, w), vec!["aa bb", "cc"]);
        assert_eq!(wrap("abcdefgh ij", 5.0, w), vec!["abcdefgh", "ij"]);
        assert_eq!(wrap("", 5.0, w), Vec::<String>::new());
    }

    /// Only the newest lines are kept.
    #[test]
    fn the_chat_keeps_its_newest_lines() {
        let mut c = Chat::default();
        for i in 0..10 {
            let line = cj_net::ChatLine {
                from: "a".to_owned(),
                slot: Some(0),
                text: i.to_string(),
            };
            c.push(line, f64::from(i));
        }
        assert_eq!(c.lines.len(), CHAT_KEEP);
        assert_eq!(c.lines.back().map(|l| l.1.text.as_str()), Some("9"));
    }
}
