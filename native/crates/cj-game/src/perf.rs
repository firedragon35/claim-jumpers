//! The frame-time overlay (F3): how long frames take, the worst one
//! lately, and what is on screen that costs time. Hidden by default and
//! not saved; it changes nothing it measures.

use crate::look::{col, rgba, text_left, Look};
use macroquad::prelude::*;
use std::collections::VecDeque;

/// Seconds of recent frames the figures cover.
const WINDOW: f32 = 2.0;
/// A frame slower than this (ms) shows the worst figure in warning colour:
/// below 60 fps.
const SLOW_MS: f32 = 1000.0 / 60.0;

/// Recent frame times and whether the overlay is showing.
pub struct Perf {
    pub on: bool,
    /// Real (unclamped) frame times in seconds, oldest first, covering
    /// about the last WINDOW seconds.
    times: VecDeque<f32>,
    total: f32,
}

/// What the overlay reports about the last WINDOW seconds.
#[derive(Debug, PartialEq)]
pub struct Stats {
    /// Mean frame time, ms.
    pub avg_ms: f32,
    /// Frames per second from the mean.
    pub fps: f32,
    /// Slowest single frame, ms.
    pub worst_ms: f32,
}

impl Perf {
    pub fn new() -> Perf {
        Perf {
            on: false,
            times: VecDeque::new(),
            total: 0.0,
        }
    }

    /// Note one frame's time `dt` (seconds), dropping frames older than
    /// the window. Recorded even while hidden, so the figures are ready
    /// the moment it opens.
    pub fn record(&mut self, dt: f32) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        self.times.push_back(dt);
        self.total += dt;
        while self.total > WINDOW && self.times.len() > 1 {
            if let Some(old) = self.times.pop_front() {
                self.total -= old;
            }
        }
    }

    /// Figures over the window, or None before any frame is recorded.
    pub fn stats(&self) -> Option<Stats> {
        if self.times.is_empty() {
            return None;
        }
        let avg = self.total / self.times.len() as f32;
        let worst = self.times.iter().copied().fold(0.0, f32::max);
        Some(Stats {
            avg_ms: avg * 1000.0,
            fps: 1.0 / avg,
            worst_ms: worst * 1000.0,
        })
    }

    /// Draw the panel at the left edge, mid-height (clear of the HUD's
    /// corners), in window units. `particles` and `effects` are what the
    /// frame drew.
    pub fn draw(&self, look: &Look, view: (f32, f32), particles: usize, effects: bool) {
        let Some(s) = self.stats().filter(|_| self.on) else {
            return;
        };
        let (x, y) = (16.0, (view.1 * 0.42).round());
        let font = &look.fonts.mono;
        let rows = [
            (format!("{:.0} fps  {:.1} ms", s.fps, s.avg_ms), col::TEXT),
            (
                format!("worst {:.1} ms (last {WINDOW:.0} s)", s.worst_ms),
                if s.worst_ms > SLOW_MS {
                    col::DANGER
                } else {
                    col::TEXT
                },
            ),
            (format!("particles {particles}"), col::MUTED),
            (
                format!("effects {}", if effects { "on" } else { "off" }),
                col::MUTED,
            ),
        ];
        draw_rectangle(
            x - 8.0,
            y - 6.0,
            230.0,
            18.0 * rows.len() as f32 + 12.0,
            rgba(0x05070d, 0.75),
        );
        for (i, (text, rgb)) in rows.iter().enumerate() {
            text_left(
                text,
                x,
                y + 18.0 * i as f32,
                13.0,
                font,
                crate::look::hex(*rgb),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The figures cover only the last couple of seconds: an old hitch
    /// ages out, a new one shows as the worst frame, and bad timings are
    /// ignored.
    #[test]
    fn figures_cover_the_recent_window() {
        let mut p = Perf::new();
        assert_eq!(p.stats(), None);
        p.record(0.050); // a hitch
        for _ in 0..240 {
            p.record(1.0 / 120.0); // two seconds of 120 fps
        }
        let s = p.stats().expect("frames recorded");
        assert!((s.fps - 120.0).abs() < 0.5, "{s:?}");
        assert!(
            (s.worst_ms - 1000.0 / 120.0).abs() < 0.01,
            "the hitch aged out: {s:?}"
        );

        p.record(0.040);
        assert!((p.stats().expect("frames").worst_ms - 40.0).abs() < 0.01);

        for bad in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            p.record(bad);
        }
        assert!(p.stats().expect("frames").avg_ms.is_finite());
    }
}
