//! A small immediate-mode menu kit for the title, pause, report and shop
//! screens (the browser's HTML cards, styled by src/styles.css).
//!
//! Screens lay themselves out in "design pixels" (the CSS pixels of the
//! browser's layout) and are scaled down to fit the window, as the
//! browser's fitScreens does; they are never scaled up.
//!
//! Buttons are declared each frame. Keyboard and gamepad move focus
//! spatially to the nearest button in the pressed direction (moveFocus in
//! src/js/12-input.js, same scoring and key-repeat), and Enter, Space or A
//! presses the focused one. The mouse hovers and clicks. The focus ring
//! shows only while navigating by keyboard or pad, as in the browser.

use crate::look::{col, rgba, Look};
use macroquad::prelude::*;

/// One frame of menu input, merged from keyboard, pad and mouse.
#[derive(Clone, Copy, Debug, Default)]
pub struct MenuInput {
    /// Focus direction this frame (already key-repeated), if any.
    pub nav: Option<(f32, f32)>,
    /// Press the focused button.
    pub activate: bool,
    /// Step the screen's tab strip (LB / RB).
    pub tab: i32,
    /// Cancel / back (Esc, B).
    pub back: bool,
    /// The pad's Start: presses the screen's primary button.
    pub start: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Style {
    /// Hazard-amber call to action (button.primary).
    Primary,
    /// Outlined secondary (button.ghost).
    Ghost,
    /// Small shop action (button.act); `Buy` is the amber variant.
    Act,
    Buy,
    /// Difficulty picker (button.mode); `pressed` shows the choice.
    Mode {
        pressed: bool,
    },
    /// A tab in a strip (button.tab).
    Tab {
        selected: bool,
    },
    /// A hardpoint on the shop bench (.slot).
    Slot {
        filled: bool,
    },
    /// A belt card on the sector map (.belt): a solid edge, cable when picked.
    Belt {
        picked: bool,
    },
}

struct Btn {
    id: String,
    rect: Rect,
    enabled: bool,
    primary: bool,
}

pub struct Ui {
    focus: Option<String>,
    prev: Vec<Btn>,
    cur: Vec<Btn>,
    pressed: Option<String>,
    /// Keyboard or pad navigation is in use (show the focus ring).
    pub pad_nav: bool,
    last_mouse: Vec2,
    /// Design-to-screen mapping for this frame.
    scale: f32,
    origin: Vec2,
    dpi: f32,
    /// Held direction and when it repeats next (seconds).
    held: Option<(f32, f32)>,
    repeat_at: f64,
}

impl Ui {
    pub fn new() -> Ui {
        Ui {
            focus: None,
            prev: Vec::new(),
            cur: Vec::new(),
            pressed: None,
            pad_nav: false,
            last_mouse: Vec2::ZERO,
            scale: 1.0,
            origin: Vec2::ZERO,
            dpi: 1.0,
            held: None,
            repeat_at: 0.0,
        }
    }

    /// Forget focus (a new screen opened): the next frame focuses its
    /// primary button, as focusFirst does.
    pub fn reset(&mut self) {
        self.focus = None;
        self.prev.clear();
    }

    /// Turn held directions into focus moves: once on press, then after
    /// 380 ms, then every 160 ms (menuNav).
    pub fn repeat(&mut self, dir: Option<(f32, f32)>) -> Option<(f32, f32)> {
        let now = get_time();
        let out = match dir {
            Some(d) if Some(d) != self.held => {
                self.repeat_at = now + 0.38;
                Some(d)
            }
            Some(d) if now >= self.repeat_at => {
                self.repeat_at = now + 0.16;
                Some(d)
            }
            _ => None,
        };
        self.held = dir;
        out
    }

    /// Start a frame for a screen `size` design pixels large in a window of
    /// `view` logical pixels. Applies last frame's navigation and clicks.
    pub fn begin(&mut self, input: &MenuInput, size: (f32, f32), view: (f32, f32), dpi: f32) {
        self.scale = (view.0 / size.0).min(view.1 / size.1).min(1.0);
        self.origin = vec2(
            (view.0 - size.0 * self.scale) / 2.0,
            (view.1 - size.1 * self.scale) / 2.0,
        );
        self.dpi = dpi;
        set_camera(&crate::draw::display_camera(Rect::new(
            -self.origin.x / self.scale,
            -self.origin.y / self.scale,
            view.0 / self.scale,
            view.1 / self.scale,
        )));
        self.pressed = None;
        let enabled = |b: &&Btn| b.enabled;
        if self
            .focus
            .as_ref()
            .is_none_or(|f| !self.prev.iter().any(|b| &b.id == f && b.enabled))
        {
            // focusFirst: the primary button, else the first enabled one.
            let first = self.prev.iter().filter(enabled).find(|b| b.primary);
            let first = first.or_else(|| self.prev.iter().find(|b| b.enabled));
            self.focus = first.map(|b| b.id.clone());
        }
        if let Some((dx, dy)) = input.nav {
            self.pad_nav = true;
            self.move_focus(dx, dy);
        }
        if input.activate {
            self.pressed = self.focus.clone();
        }
        if input.start {
            self.pressed = self
                .prev
                .iter()
                .filter(enabled)
                .find(|b| b.primary)
                .map(|b| b.id.clone());
        }
        // Mouse: moving it hides the focus ring; a click presses.
        let m = self.mouse();
        if m != self.last_mouse {
            self.last_mouse = m;
            self.pad_nav = false;
        }
        if is_mouse_button_pressed(MouseButton::Left) {
            if let Some(b) = self
                .prev
                .iter()
                .filter(enabled)
                .find(|b| b.rect.contains(m))
            {
                self.pressed = Some(b.id.clone());
                self.focus = Some(b.id.clone());
            }
        }
        std::mem::swap(&mut self.prev, &mut self.cur);
        self.cur.clear();
    }

    /// The mouse in design pixels.
    pub fn mouse(&self) -> Vec2 {
        (Vec2::from(mouse_position()) - self.origin) / self.scale
    }

    /// Text rasterizes at its on-screen pixel size.
    pub fn raster(&self) -> f32 {
        self.scale * self.dpi
    }

    /// moveFocus: the closest enabled button roughly in direction (dx, dy).
    fn move_focus(&mut self, dx: f32, dy: f32) {
        let Some(cur) = self
            .prev
            .iter()
            .find(|b| Some(&b.id) == self.focus.as_ref())
        else {
            return;
        };
        let c = cur.rect.center();
        let mut best: Option<(&Btn, f32)> = None;
        for b in self.prev.iter().filter(|b| b.enabled && b.id != cur.id) {
            let o = b.rect.center() - c;
            let along = o.x * dx + o.y * dy;
            if along <= 4.0 {
                continue; // behind or level with the current button
            }
            let across = (o.x * dy - o.y * dx).abs();
            let score = along + across * 2.5; // prefer straight-line neighbours
            if best.is_none_or(|(_, s)| score < s) {
                best = Some((b, score));
            }
        }
        if let Some((b, _)) = best {
            self.focus = Some(b.id.clone());
        }
    }

    pub fn focus_is(&self, id: &str) -> bool {
        self.focus.as_deref() == Some(id)
    }

    /// Declare a button; true if it was pressed this frame.
    pub fn button(
        &mut self,
        look: &Look,
        id: &str,
        r: Rect,
        style: Style,
        label: &str,
        enabled: bool,
    ) -> bool {
        let hover = enabled && r.contains(self.mouse()) && !self.pad_nav;
        let focused = self.focus_is(id);
        draw_button(look, self, r, style, label, enabled, hover);
        if focused && self.pad_nav {
            // button:focus-visible: a 2 px cable ring, 3 px out (inset on tabs).
            let o = if matches!(style, Style::Tab { .. }) {
                -2.0
            } else {
                3.0
            };
            draw_rectangle_lines(
                r.x - o,
                r.y - o,
                r.w + 2.0 * o,
                r.h + 2.0 * o,
                2.0,
                Color::from_hex(col::CABLE),
            );
        }
        self.cur.push(Btn {
            id: id.to_owned(),
            rect: r,
            enabled,
            primary: style == Style::Primary,
        });
        enabled && self.pressed.as_deref() == Some(id)
    }

    /// A button sized to its label at (x, y); returns (pressed, its rect).
    // Layout primitives take their position, style and label as plain
    // arguments, like the canvas calls they stand in for.
    #[allow(clippy::too_many_arguments)]
    pub fn button_at(
        &mut self,
        look: &Look,
        id: &str,
        x: f32,
        y: f32,
        style: Style,
        label: &str,
        enabled: bool,
    ) -> (bool, Rect) {
        let r = button_rect(look, x, y, style, label);
        (self.button(look, id, r, style, label, enabled), r)
    }
}

/// Font, size, padding and case for each button style (styles.css).
fn button_metrics(style: Style) -> (Which, f32, f32, f32, bool) {
    match style {
        Style::Primary => (Which::Display, 26.0, 26.0, 10.0, true),
        Style::Ghost => (Which::Mono, 13.0, 14.0, 10.0, false),
        Style::Act | Style::Buy => (Which::Mono, 12.0, 12.0, 8.0, true),
        Style::Mode { .. } => (Which::Mono, 13.0, 12.0, 10.0, true),
        Style::Tab { .. } => (Which::Mono, 12.0, 14.0, 9.0, true),
        Style::Slot { .. } | Style::Belt { .. } => (Which::Body, 15.0, 10.0, 8.0, false),
    }
}

pub fn button_rect(look: &Look, x: f32, y: f32, style: Style, label: &str) -> Rect {
    let (which, size, px, py, upper) = button_metrics(style);
    let text = if upper {
        label.to_uppercase()
    } else {
        label.to_owned()
    };
    let spaced = text.chars().count() as f32 * letter_spacing(style, size);
    let w = text_w(look, &text, size, which) + spaced + px * 2.0;
    Rect::new(x, y, w, size * 1.25 + py * 2.0)
}

fn draw_button(
    look: &Look,
    ui: &Ui,
    r: Rect,
    style: Style,
    label: &str,
    enabled: bool,
    hover: bool,
) {
    let a = if enabled { 1.0 } else { 0.4 };
    let (which, size, px, _, upper) = button_metrics(style);
    let text = if upper {
        label.to_uppercase()
    } else {
        label.to_owned()
    };
    let edge = Color::from_hex(col::PANEL_EDGE);
    let (fill, border, fg) = match style {
        Style::Primary => {
            let c = if hover { 0xffc24d } else { col::HAZARD };
            (Some(rgba(c, a)), None, rgba(0x120c02, a))
        }
        Style::Buy => (
            Some(rgba(col::HAZARD, a)),
            Some(rgba(col::HAZARD, a)),
            rgba(0x120c02, a),
        ),
        Style::Ghost | Style::Act => (
            None,
            Some(if hover { rgba(col::MUTED, a) } else { edge }),
            rgba(col::TEXT, a),
        ),
        Style::Mode { pressed: true } => (
            Some(rgba(col::CABLE, 0.12)),
            Some(Color::from_hex(col::CABLE)),
            Color::from_hex(col::CABLE),
        ),
        Style::Mode { pressed: false } => (
            None,
            Some(edge),
            rgba(if hover { col::TEXT } else { col::MUTED }, a),
        ),
        Style::Tab { selected: true } => (
            Some(rgba(0x090d15, 0.86)),
            Some(edge),
            Color::from_hex(col::HAZARD),
        ),
        Style::Tab { selected: false } => (
            None,
            None,
            rgba(if hover { col::TEXT } else { col::MUTED }, a),
        ),
        Style::Slot { filled: true } => (
            Some(Color::from_hex(0x0c1220)),
            Some(Color::from_hex(if hover {
                col::DANGER
            } else {
                col::HAZARD
            })),
            Color::from_hex(col::TEXT),
        ),
        Style::Slot { filled: false } => (
            Some(Color::from_hex(0x0c1220)),
            None,
            Color::from_hex(col::MUTED),
        ),
        Style::Belt { picked } => (
            Some(Color::from_hex(0x0c1220)),
            Some(if picked {
                Color::from_hex(col::CABLE)
            } else {
                edge
            }),
            Color::from_hex(col::TEXT),
        ),
    };
    if let Some(f) = fill {
        draw_rectangle(r.x, r.y, r.w, r.h, f);
    }
    match style {
        // An empty hardpoint has a dashed outline.
        Style::Slot { filled: false } => dashed_rect(r, edge),
        // A selected tab has no bottom edge, so it opens into its panel.
        Style::Tab { selected: true } => {
            draw_line(r.x, r.y + r.h, r.x, r.y, 1.0, edge);
            draw_line(r.x, r.y, r.x + r.w, r.y, 1.0, edge);
            draw_line(r.x + r.w, r.y, r.x + r.w, r.y + r.h, 1.0, edge);
        }
        _ => {
            if let Some(b) = border {
                draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, b);
            }
        }
    }
    let spacing = letter_spacing(style, size);
    let tw = text_w(look, &text, size, which) + text.chars().count() as f32 * spacing;
    let tx = match style {
        Style::Slot { .. } | Style::Belt { .. } => r.x + px,
        _ => r.x + (r.w - tw) / 2.0,
    };
    let ty = r.y + (r.h - size * 1.25) / 2.0 + size * 0.1;
    text_spaced(look, ui, &text, tx, ty, size, which, fg, spacing);
}

/// CSS letter-spacing per button style, in pixels.
fn letter_spacing(style: Style, size: f32) -> f32 {
    match style {
        Style::Primary => size * 0.04,
        Style::Mode { .. } | Style::Act | Style::Buy => size * 0.08,
        Style::Tab { .. } => size * 0.1,
        _ => 0.0,
    }
}

/// Level pips: `filled` solid dots then hollow ones, `total` in all (the
/// ● and ○ of .pips; JetBrains Mono has no ○, so they are drawn).
pub fn pips(x: f32, y: f32, filled: usize, total: usize, size: f32, color: Color) {
    let r = size * 0.32;
    let step = size * 0.75;
    for i in 0..total {
        let cx = x + r + i as f32 * step;
        if i < filled {
            draw_circle(cx, y, r, color);
        } else {
            draw_circle_lines(cx, y, r - 0.5, 1.0, color);
        }
    }
}

fn dashed_rect(r: Rect, c: Color) {
    let corners = [
        vec2(r.x, r.y),
        vec2(r.x + r.w, r.y),
        vec2(r.x + r.w, r.y + r.h),
        vec2(r.x, r.y + r.h),
    ];
    crate::draw::dashed(&corners, true, 4.0, 3.0, 0.0, 1.0, c);
}

/// Which of the game's fonts.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Which {
    /// Big Shoulders Stencil Display Black: titles and big numbers.
    Display,
    /// Barlow Semi Condensed: body text.
    Body,
    /// JetBrains Mono Bold: labels and buttons.
    Mono,
    /// JetBrains Mono Medium: small print.
    MonoLight,
}

fn font(look: &Look, w: Which) -> &Font {
    match w {
        Which::Display => &look.fonts.display,
        Which::Body => &look.fonts.body,
        Which::Mono => &look.fonts.mono,
        Which::MonoLight => &look.fonts.mono_light,
    }
}

/// Width of `text` in design pixels.
pub fn text_w(look: &Look, text: &str, size: f32, w: Which) -> f32 {
    measure_text(text, Some(font(look, w)), size.round() as u16, 1.0).width
}

/// Text with its top-left at (x, y), rasterized crisply at the screen scale.
#[allow(clippy::too_many_arguments)]
pub fn text(
    look: &Look,
    ui: &Ui,
    s: &str,
    x: f32,
    y: f32,
    size: f32,
    w: Which,
    color: Color,
) -> f32 {
    let rs = ui.raster();
    let px = (size * rs).round().clamp(1.0, 400.0) as u16;
    let f = font(look, w);
    let dims = measure_text(s, Some(f), px, 1.0 / rs);
    // Glyph tops sit about 0.8 of the font size below the line top.
    draw_text_ex(
        s,
        x,
        y + size * 0.85,
        TextParams {
            font: Some(f),
            font_size: px,
            font_scale: 1.0 / rs,
            color,
            ..Default::default()
        },
    );
    dims.width
}

/// Letter-spaced text (CSS letter-spacing), drawn a character at a time.
#[allow(clippy::too_many_arguments)]
pub fn text_spaced(
    look: &Look,
    ui: &Ui,
    s: &str,
    x: f32,
    y: f32,
    size: f32,
    w: Which,
    color: Color,
    spacing: f32,
) -> f32 {
    if spacing == 0.0 {
        return text(look, ui, s, x, y, size, w, color);
    }
    let mut cx = x;
    for ch in s.chars() {
        let c = ch.to_string();
        text(look, ui, &c, cx, y, size, w, color);
        cx += text_w(look, &c, size, w) + spacing;
    }
    cx - x
}

/// An uppercase, letter-spaced label (.label / .eyebrow: 11 px, 0.14 em).
pub fn label(look: &Look, ui: &Ui, s: &str, x: f32, y: f32, color: Color) -> f32 {
    text_spaced(
        look,
        ui,
        &s.to_uppercase(),
        x,
        y,
        11.0,
        Which::MonoLight,
        color,
        11.0 * 0.14,
    )
}

/// Break `s` into lines no wider than `max_w`.
pub fn wrap(look: &Look, s: &str, size: f32, w: Which, max_w: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for para in s.split('\n') {
        let mut line = String::new();
        for word in para.split_whitespace() {
            let trial = if line.is_empty() {
                word.to_owned()
            } else {
                format!("{line} {word}")
            };
            if !line.is_empty() && text_w(look, &trial, size, w) > max_w {
                lines.push(std::mem::take(&mut line));
                line = word.to_owned();
            } else {
                line = trial;
            }
        }
        lines.push(line);
    }
    lines
}

/// A wrapped paragraph; returns its height.
#[allow(clippy::too_many_arguments)]
pub fn paragraph(
    look: &Look,
    ui: &Ui,
    s: &str,
    x: f32,
    y: f32,
    max_w: f32,
    size: f32,
    w: Which,
    color: Color,
    line_h: f32,
) -> f32 {
    let lines = wrap(look, s, size, w, max_w);
    for (i, l) in lines.iter().enumerate() {
        text(look, ui, l, x, y + i as f32 * size * line_h, size, w, color);
    }
    lines.len() as f32 * size * line_h
}

/// Height a wrapped paragraph will take.
pub fn paragraph_h(look: &Look, s: &str, max_w: f32, size: f32, w: Which, line_h: f32) -> f32 {
    wrap(look, s, size, w, max_w).len() as f32 * size * line_h
}

/// A work-order card: dark panel, edge, and hazard tape along the top.
pub fn card(r: Rect) {
    draw_rectangle(
        r.x,
        r.y,
        r.w,
        r.h,
        Color::new(9.0 / 255.0, 13.0 / 255.0, 21.0 / 255.0, 0.86),
    );
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, Color::from_hex(col::PANEL_EDGE));
    // repeating-linear-gradient(-45deg, hazard 0 10px, #1a1408 10px 20px), 6 px tall.
    let (dark, amber) = (Color::from_hex(0x1a1408), Color::from_hex(col::HAZARD));
    draw_rectangle(r.x, r.y, r.w, 6.0, dark);
    let period = 20.0 * std::f32::consts::SQRT_2;
    let mut x = r.x - 6.0;
    while x < r.x + r.w {
        // One amber band: a parallelogram leaning right, clipped to the card.
        let w = 10.0 * std::f32::consts::SQRT_2;
        let (x0, x1) = (x.max(r.x), (x + w).min(r.x + r.w));
        let (b0, b1) = ((x + 6.0).max(r.x), (x + 6.0 + w).min(r.x + r.w));
        if x1 > x0 || b1 > b0 {
            let p = [
                vec2(x0, r.y + 6.0),
                vec2(x1, r.y + 6.0),
                vec2(b1, r.y),
                vec2(b0, r.y),
            ];
            draw_triangle(p[0], p[1], p[2], amber);
            draw_triangle(p[0], p[2], p[3], amber);
        }
        x += period;
    }
}

/// The dimmed backdrop behind every menu screen (.screen's radial wash).
pub fn backdrop(view: (f32, f32)) {
    crate::draw::screen_camera(view);
    draw_rectangle(
        0.0,
        0.0,
        view.0,
        view.1,
        Color::new(5.0 / 255.0, 7.0 / 255.0, 13.0 / 255.0, 0.75),
    );
}

/// dl.ledger: muted labels, right-aligned values, 13 px mono, columns
/// sized to their widest entry (gap 2 px rows, 18 px columns). Returns the
/// block's size. A value's colour marks it (.pos amber, .neg danger).
pub fn ledger(look: &Look, ui: &Ui, x: f32, y: f32, rows: &[(&str, String, u32)]) -> (f32, f32) {
    let size = 13.0;
    let lw = rows
        .iter()
        .map(|r| text_w(look, r.0, size, Which::MonoLight))
        .fold(0.0, f32::max);
    let vw = rows
        .iter()
        .map(|r| text_w(look, &r.1, size, Which::MonoLight))
        .fold(0.0, f32::max);
    let line = size * 1.4 + 2.0;
    for (i, (l, v, c)) in rows.iter().enumerate() {
        let ry = y + i as f32 * line;
        text(
            look,
            ui,
            l,
            x,
            ry,
            size,
            Which::MonoLight,
            Color::from_hex(col::MUTED),
        );
        let w = text_w(look, v, size, Which::MonoLight);
        text(
            look,
            ui,
            v,
            x + lw + 18.0 + vw - w,
            ry,
            size,
            Which::MonoLight,
            Color::from_hex(*c),
        );
    }
    (lw + 18.0 + vw, rows.len() as f32 * line)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ui_with(buttons: &[(&str, f32, f32, bool)]) -> Ui {
        let mut u = Ui::new();
        u.prev = buttons
            .iter()
            .map(|&(id, x, y, enabled)| Btn {
                id: id.to_owned(),
                rect: Rect::new(x, y, 100.0, 40.0),
                enabled,
                primary: false,
            })
            .collect();
        u
    }

    #[test]
    fn focus_moves_to_the_nearest_button_in_that_direction() {
        // A shop-like layout: two cards side by side, a row of actions below.
        let mut u = ui_with(&[
            ("left", 0.0, 0.0, true),
            ("right", 300.0, 0.0, true),
            ("far-right", 600.0, 10.0, true),
            ("start", 0.0, 200.0, true),
            ("pay", 300.0, 200.0, true),
        ]);
        u.focus = Some("left".into());
        u.move_focus(1.0, 0.0);
        assert_eq!(
            u.focus.as_deref(),
            Some("right"),
            "straight right, not the farther one"
        );
        u.move_focus(0.0, 1.0);
        assert_eq!(u.focus.as_deref(), Some("pay"), "straight down");
        u.move_focus(-1.0, 0.0);
        assert_eq!(u.focus.as_deref(), Some("start"));
        u.move_focus(-1.0, 0.0);
        assert_eq!(
            u.focus.as_deref(),
            Some("start"),
            "nothing further left: focus stays"
        );
    }

    #[test]
    fn disabled_buttons_are_skipped() {
        let mut u = ui_with(&[
            ("a", 0.0, 0.0, true),
            ("b", 200.0, 0.0, false),
            ("c", 400.0, 0.0, true),
        ]);
        u.focus = Some("a".into());
        u.move_focus(1.0, 0.0);
        assert_eq!(u.focus.as_deref(), Some("c"));
    }
}
