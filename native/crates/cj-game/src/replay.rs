//! The shift highlight replay (src/js/10d-presentation.js). A ring buffer
//! keeps a light snapshot of the scene round the tug at REPLAY_HZ. When
//! something worth showing happens (a smash, a boss, the vault, a big
//! delivery), the best one so far is captured REPLAY_TAIL seconds later, so
//! the clip shows the build-up and the payoff. The report loops it.

use crate::look::col;
use cj_sim::entities::Item;
use cj_sim::items::ItemType;
use cj_sim::World;
use macroquad::prelude::*;
use std::collections::VecDeque;

const REPLAY_HZ: f64 = 20.0;
const REPLAY_SECONDS: f64 = 6.0;
const REPLAY_TAIL: f64 = 1.5;
/// Only things this close to the tug are recorded.
const REPLAY_RADIUS: f64 = 900.0;
/// World width shown in the replay.
const REPLAY_VIEW: f32 = 1400.0;

/// One snapshot: positions, angles, radii and colours, nothing else.
#[derive(Clone)]
pub struct Frame {
    ship: Option<(f32, f32, f32)>,
    cam: (f32, f32),
    chain: Vec<(f32, f32, f32, u32)>,
    items: Vec<(f32, f32, f32, u32)>,
    foes: Vec<(f32, f32, f32, u32, f32)>,
    /// A crude "something exploded" flag for a flash.
    flash: bool,
}

#[derive(Clone)]
pub struct Highlight {
    pub text: String,
    pub frames: Vec<Frame>,
}

/// itemColour: the grade's tint, else a colour per type.
fn item_colour(it: &Item) -> u32 {
    it.tint.unwrap_or(match it.kind {
        ItemType::Ore | ItemType::Ingot => col::ORE,
        ItemType::Reactor => col::REACTOR,
        ItemType::Engine => col::ENGINE,
        ItemType::Blackbox => col::HAZARD,
        ItemType::Pod => col::COLLECTOR,
        ItemType::Keycard => col::CABLE,
        ItemType::Core => col::DANGER,
        _ => col::STEEL,
    })
}

#[derive(Default)]
pub struct Recorder {
    rec: VecDeque<Frame>,
    rec_t: f64,
    score: f64,
    pending: Option<(String, f64)>,
    pub highlight: Option<Highlight>,
}

impl Recorder {
    /// Snapshot the scene round the tug (called every step; samples at REPLAY_HZ).
    pub fn record(&mut self, w: &World, dt: f64, particles: usize) {
        self.rec_t -= dt;
        if self.rec_t > 0.0 {
            return;
        }
        self.rec_t = 1.0 / REPLAY_HZ;
        let s = &w.me().ship;
        let near = |x: f64, y: f64| (x - s.b.x).hypot(y - s.b.y) < REPLAY_RADIUS;
        let f = |v: f64| v as f32;
        let chain = w
            .me()
            .chain
            .iter()
            .filter_map(|id| w.items.iter().find(|it| it.id == *id))
            .map(|it| (f(it.b.x), f(it.b.y), f(it.b.r), item_colour(it)))
            .collect();
        let items = w
            .items
            .iter()
            .filter(|it| !it.towed && !it.dead && near(it.b.x, it.b.y))
            .take(40)
            .map(|it| (f(it.b.x), f(it.b.y), f(it.b.r), item_colour(it)))
            .collect();
        let mut foes: Vec<(f32, f32, f32, u32, f32)> = Vec::new();
        for j in w.jumpers.iter().filter(|j| !j.dead && near(j.b.x, j.b.y)) {
            let c = if j.boss.is_some() {
                col::HAZARD
            } else {
                col::JUMPER
            };
            foes.push((f(j.b.x), f(j.b.y), f(j.angle), c, f(j.b.r)));
        }
        for c in w
            .collectors
            .iter()
            .filter(|c| !c.dead && near(c.b.x, c.b.y))
        {
            foes.push((f(c.b.x), f(c.b.y), f(c.angle), col::COLLECTOR, f(c.b.r)));
        }
        for sh in w
            .sheriffs
            .iter()
            .filter(|sh| !sh.dead && near(sh.b.x, sh.b.y))
        {
            foes.push((f(sh.b.x), f(sh.b.y), f(sh.angle), col::LAW, f(sh.b.r)));
        }
        self.rec.push_back(Frame {
            ship: s.alive.then(|| (f(s.b.x), f(s.b.y), f(s.angle))),
            cam: (f(s.b.x), f(s.b.y)),
            chain,
            items,
            foes,
            flash: particles > 60,
        });
        if self.rec.len() > (REPLAY_HZ * REPLAY_SECONDS) as usize {
            self.rec.pop_front();
        }
        if self.pending.as_ref().is_some_and(|p| w.t >= p.1) {
            self.capture();
        }
    }

    /// Something worth replaying happened; keep it if it beats this shift's best.
    pub fn note(&mut self, score: f64, text: &str, t: f64) {
        if score <= self.score {
            return;
        }
        self.score = score;
        self.pending = Some((text.to_owned(), t + REPLAY_TAIL));
    }

    pub fn capture(&mut self) {
        if let Some((text, _)) = self.pending.take() {
            self.highlight = Some(Highlight {
                text,
                frames: self.rec.iter().cloned().collect(),
            });
        }
    }

    /// The shift is over: capture anything still pending and hand it over.
    pub fn finish(&mut self) -> Option<Highlight> {
        self.capture();
        let h = self.highlight.take();
        *self = Recorder::default();
        h
    }
}

/// Draw the replay frame for `secs` since the clip started, in `r`: play
/// through, hold the last frame for a second, then loop.
pub fn draw(h: &Highlight, r: Rect, secs: f64) {
    let n = h.frames.len();
    if n == 0 {
        return;
    }
    let total = n + REPLAY_HZ as usize;
    let i = ((secs * REPLAY_HZ).floor() as usize % total).min(n - 1);
    let fr = &h.frames[i];
    let k = r.w / REPLAY_VIEW;
    let px = |x: f32| r.x + r.w / 2.0 + (x - fr.cam.0) * k;
    let py = |y: f32| r.y + r.h / 2.0 + (y - fr.cam.1) * k;
    // The browser's canvas clips to its box; here anything centred outside
    // it is simply not drawn.
    let inside = |x: f32, y: f32| r.contains(vec2(px(x), py(y)));
    draw_rectangle(r.x, r.y, r.w, r.h, crate::look::hex(col::VOID));
    if fr.flash {
        draw_rectangle(
            r.x,
            r.y,
            r.w,
            r.h,
            Color::new(1.0, 244.0 / 255.0, 220.0 / 255.0, 0.12),
        );
    }
    let rgba = |c: u32, a: f32| {
        let c = crate::look::hex(c);
        Color::new(c.r, c.g, c.b, a)
    };
    for &(x, y, rad, c) in fr.items.iter().filter(|i| inside(i.0, i.1)) {
        draw_circle(px(x), py(y), (rad * k).max(1.5), rgba(c, 0.6));
    }
    // Cable from the tug through every towed piece.
    if let Some((sx, sy, _)) = fr.ship {
        let mut prev = (px(sx), py(sy));
        for &(x, y, _, _) in &fr.chain {
            let p = (px(x), py(y));
            if inside(x, y) {
                draw_line(prev.0, prev.1, p.0, p.1, 1.0, crate::look::hex(col::CABLE));
            }
            prev = p;
        }
    }
    for &(x, y, rad, c) in fr.chain.iter().filter(|i| inside(i.0, i.1)) {
        draw_circle(px(x), py(y), (rad * k).max(2.0), crate::look::hex(c));
    }
    let arrow = |x: f32, y: f32, a: f32, size: f32, c: u32| {
        let (co, si) = (a.cos(), a.sin());
        let p = |lx: f32, ly: f32| vec2(px(x) + lx * co - ly * si, py(y) + lx * si + ly * co);
        let (p0, p1, p2, p3) = (
            p(size, 0.0),
            p(-size * 0.7, -size * 0.7),
            p(-size * 0.4, 0.0),
            p(-size * 0.7, size * 0.7),
        );
        // A concave dart: two triangles meeting at the notch.
        draw_triangle(p0, p1, p2, crate::look::hex(c));
        draw_triangle(p0, p2, p3, crate::look::hex(c));
    };
    for &(x, y, a, c, rad) in fr.foes.iter().filter(|f| inside(f.0, f.1)) {
        arrow(x, y, a, (rad * k * 1.1).max(4.0), c);
    }
    if let Some((x, y, a)) = fr.ship {
        arrow(x, y, a, 7.0, col::HAZARD);
    }
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 1.0, crate::look::hex(col::PANEL_EDGE));
}
