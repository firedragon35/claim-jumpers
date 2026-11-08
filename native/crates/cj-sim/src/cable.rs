//! The tow cable (src/js/15-tow-cable-helpers.js and solveCables).
//!
//! The chain is ordered from the tug outward: chain[0] hangs off the tug,
//! chain[i] hangs off chain[i - 1]. Each link is an inextensible rope that
//! goes slack when shorter than its length, which is what lets cargo swing
//! and whip.

use crate::consts::*;
use crate::entities::{Body, CrabState, Item};
use crate::events::{Radio, Sfx, Tint};
use crate::world::World;

/// Rope between two bodies. If stretched past `len`, positions are pulled
/// together in proportion to inverse mass and the separating velocity is
/// cancelled. Returns the velocity change applied to `b` (the "jerk").
pub fn rope_constraint(a: &mut Body, b: &mut Body, len: f64) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let d = crate::js::hypot(dx, dy);
    if d <= len || d == 0.0 {
        return 0.0;
    }
    let (nx, ny) = (dx / d, dy / d);
    let (wa, wb) = (a.inv_mass(), b.inv_mass());
    let ws = wa + wb;
    if ws == 0.0 {
        return 0.0;
    }
    let corr = (d - len) / ws;
    a.x += nx * corr * wa;
    a.y += ny * corr * wa;
    b.x -= nx * corr * wb;
    b.y -= ny * corr * wb;
    let rv = (b.vx - a.vx) * nx + (b.vy - a.vy) * ny;
    if rv <= 0.0 {
        return 0.0;
    }
    let imp = rv / ws;
    a.vx += nx * imp * wa;
    a.vy += ny * imp * wa;
    b.vx -= nx * imp * wb;
    b.vy -= ny * imp * wb;
    imp * wb
}

/// Mutable references to two different items.
fn two_items(items: &mut [Item], i: usize, j: usize) -> (&mut Item, &mut Item) {
    debug_assert_ne!(i, j);
    if i < j {
        let (lo, hi) = items.split_at_mut(j);
        (&mut lo[i], &mut hi[0])
    } else {
        let (lo, hi) = items.split_at_mut(i);
        (&mut hi[0], &mut lo[j])
    }
}

impl World {
    /// Items-vector index of each chain link, tug outward.
    fn chain_indices(&self) -> Vec<usize> {
        self.chain
            .iter()
            .filter_map(|id| self.item_index(*id))
            .collect()
    }

    /// The anchor body of chain link `link`: the tug, or the previous item.
    pub(crate) fn anchor_body(&self, link: usize) -> Body {
        if link == 0 {
            self.ship.b
        } else {
            let id = self.chain[link - 1];
            self.items[self.item_index(id).expect("chain item exists")].b
        }
    }

    /// Shortest a link can reel to: the two bodies plus a little slack.
    fn min_link_len(item: &Body, anchor: &Body) -> f64 {
        item.r + anchor.r + LINK_SLACK
    }

    /// Nearest free item within hook range of the tug.
    pub fn find_hook_target(&self) -> Option<usize> {
        let s = &self.ship;
        if !s.alive {
            return None;
        }
        let mut best = None;
        let mut best_d = f64::INFINITY;
        for (i, it) in self.items.iter().enumerate() {
            if it.towed || it.dead {
                continue;
            }
            let d = it.b.dist_to(&s.b) - it.b.r;
            if d < self.mods.hook_range && d < best_d {
                best = Some(i);
                best_d = d;
            }
        }
        best
    }

    /// Attach the nearest item to the end of the chain.
    pub fn try_hook(&mut self) {
        let Some(i) = self.find_hook_target() else {
            self.sfx(Sfx::Deny);
            return;
        };
        if self.tow_mass() + self.items[i].b.mass > self.mods.winch_cap {
            self.sfx(Sfx::Deny);
            self.radio(Radio::Maxed, true, false);
            return;
        }
        self.attach_to_line(i);
        self.sfx(Sfx::Hook);
        let kind = self.items[i].kind;
        if kind == crate::items::ItemType::Reactor {
            self.radio(Radio::Reactor, true, false);
        } else if kind == crate::items::ItemType::Engine {
            self.radio(Radio::Engine, false, false);
        } else if self.tow_mass() / self.mods.winch_cap > 0.6 {
            self.radio(Radio::Heavy, false, false);
        } else {
            self.radio(Radio::Hook, false, false);
        }
    }

    /// Append an item to the end of the tow chain. The link starts at its
    /// current length; the winch reels it in over time, so a distant grab
    /// does not snap the cargo to the tug instantly.
    pub fn attach_to_line(&mut self, i: usize) {
        let anchor = self.anchor_body(self.chain.len());
        let it = &mut self.items[i];
        it.towed = true;
        it.thrown_t = 0.0;
        it.link_len = Self::min_link_len(&it.b, &anchor).max(it.b.dist_to(&anchor));
        self.chain.push(it.id);
    }

    /// Remove one item from the chain and re-link whatever hung behind it
    /// to the item's own anchor, so the train stays connected.
    pub fn remove_from_chain(&mut self, id: u32) {
        let Some(idx) = self.chain.iter().position(|c| *c == id) else {
            return;
        };
        self.chain.remove(idx);
        if let Some(i) = self.item_index(id) {
            self.items[i].towed = false;
        }
        if idx < self.chain.len() {
            let a = self.anchor_body(idx);
            let ni = self.item_index(self.chain[idx]).expect("chain item exists");
            let next = &mut self.items[ni];
            next.link_len = Self::min_link_len(&next.b, &a).max(next.b.dist_to(&a));
        }
        // A crab walking this link is back to riding its piece.
        for c in &mut self.crabs {
            if c.state == CrabState::Crawl && c.host == Some(id) {
                c.state = CrabState::Ride;
                c.ride_t = 0.0;
            }
        }
    }

    /// Detach the link at `idx` and everything behind it (a severed line).
    pub fn sever_at(&mut self, idx: usize) {
        if idx >= self.chain.len() {
            return;
        }
        let lost: Vec<u32> = self.chain.drain(idx..).collect();
        for &id in &lost {
            if let Some(i) = self.item_index(id) {
                self.items[i].towed = false;
            }
        }
        for c in &mut self.crabs {
            if c.state == CrabState::Crawl && c.host.is_some_and(|h| lost.contains(&h)) {
                c.state = CrabState::Ride;
                c.ride_t = 0.0;
            }
        }
    }

    /// Drop the last piece. It keeps its swing momentum, so this is a throw.
    pub fn drop_last(&mut self) {
        let Some(&id) = self.chain.last() else { return };
        self.sever_at(self.chain.len() - 1);
        if let Some(i) = self.item_index(id) {
            let it = &mut self.items[i];
            it.b.vx *= 1.1;
            it.b.vy *= 1.1;
            it.thrown_t = 1.6;
        }
        self.sfx(Sfx::Drop);
    }

    /// Cut the whole chain loose.
    pub fn cut_all(&mut self) {
        if self.chain.is_empty() {
            return;
        }
        self.threw_cargo = true;
        for i in self.chain_indices() {
            self.items[i].thrown_t = 1.2;
        }
        self.sever_at(0);
        self.sfx(Sfx::Drop);
        self.radio(Radio::Cut, false, false);
    }

    /// Winch reel-in, then several rope solver passes over every link.
    pub(crate) fn solve_cables(&mut self, dt: f64) {
        for it in &mut self.items {
            it.jerk = 0.0;
        }
        let links = self.chain_indices();
        for (k, &i) in links.iter().enumerate() {
            let anchor = self.anchor_body(k);
            let it = &mut self.items[i];
            let min = Self::min_link_len(&it.b, &anchor);
            if it.link_len > min {
                it.link_len = min.max(it.link_len - self.mods.reel * dt);
            }
        }
        // Rival lines: each jumper reels its hooked piece in to a fixed slack.
        let mut rival_links = Vec::new();
        for j in 0..self.jumpers.len() {
            let Some(i) = self.jumpers[j].hook.and_then(|id| self.item_index(id)) else {
                continue;
            };
            let min = self.jumpers[j].b.r + self.items[i].b.r + 60.0;
            let jm = &mut self.jumpers[j];
            if jm.link_len > min {
                jm.link_len = min.max(jm.link_len - 120.0 * dt);
            }
            rival_links.push((j, i));
        }
        // The collector holding the tug's net, if any.
        let tether = self
            .ship
            .net
            .and_then(|n| self.collector_index(n.by))
            .filter(|&c| !self.collectors[c].dead);
        // Multiple passes let corrections propagate down a long train.
        for _ in 0..CABLE_ITERATIONS {
            if self.ship.alive {
                for (k, &i) in links.iter().enumerate() {
                    let jerk = if k == 0 {
                        let it = &mut self.items[i];
                        rope_constraint(&mut self.ship.b, &mut it.b, it.link_len)
                    } else {
                        let (a, it) = two_items(&mut self.items, links[k - 1], i);
                        rope_constraint(&mut a.b, &mut it.b, it.link_len)
                    };
                    self.items[i].jerk += jerk;
                }
            }
            for &(j, i) in &rival_links {
                let jm = &mut self.jumpers[j];
                rope_constraint(&mut jm.b, &mut self.items[i].b, jm.link_len);
            }
            // The collector's tether: the same rope as cargo, so your
            // cargo's mass anchors you.
            if let Some(c) = tether.filter(|_| self.ship.alive) {
                let cl = &mut self.collectors[c];
                rope_constraint(&mut cl.b, &mut self.ship.b, cl.tether);
            }
            self.partner_constraint();
        }
        for c in &mut self.collectors {
            if c.state == crate::opposition::CollectorState::Tow && c.tether > 160.0 {
                c.tether = (c.tether - 90.0 * dt).max(160.0);
            }
        }
        // Smoothed tension drives the cable's colour; big jerks rattle the cargo.
        let mut rattle = false;
        for &i in &links {
            let it = &mut self.items[i];
            it.tension += ((it.jerk / 30.0).min(1.0) - it.tension) * 0.2;
            rattle |= it.jerk > RATTLE_JERK;
        }
        if rattle {
            self.sfx(Sfx::Rattle);
        }
    }

    /// Salvage grinder (grindLast): turn the last piece on the line into
    /// heavy slugs, two per tonne. Its sale value is gone. A reactor
    /// detonates in the grinder.
    pub fn grind_last(&mut self) {
        let last = self.chain.last().and_then(|id| self.item_index(*id));
        let Some(i) = last.filter(|_| self.mods.grinder && self.ship.alive) else {
            self.sfx(Sfx::Deny);
            return;
        };
        if self.items[i].def().volatile {
            self.radio(Radio::GrindReactor, true, false);
            self.detonate(i);
            return;
        }
        let slugs = (self.items[i].b.mass * 2.0).ceil() as u32;
        self.remove_item(i);
        let s = &mut self.ship;
        s.slugs += slugs;
        let (x, y, a) = (s.b.x, s.b.y, s.angle);
        self.popup(x, y - 30.0, format!("+{slugs} SLUGS"), Tint::Text);
        self.burst(
            x - a.cos() * 16.0,
            y - a.sin() * 16.0,
            16,
            Tint::Steel,
            180.0,
        );
        self.sfx(Sfx::Grind);
        self.radio(Radio::Grind, false, false);
    }

    /// Magnetic scoop (updateScoop): pulls light salvage toward the tug and
    /// hooks it on contact. The catch is that it pulls pebbles in too.
    pub(crate) fn update_scoop(&mut self, dt: f64) {
        if !self.mods.scoop || !self.ship.alive {
            return;
        }
        let s = self.ship.b;
        // Pull a body toward the tug; returns its distance (infinite if
        // out of reach, so it can never be close enough to hook).
        let pull = |b: &mut Body, strength: f64| {
            let d = b.dist_to(&s);
            if d > SCOOP_RANGE || d == 0.0 {
                return f64::INFINITY;
            }
            let k = (strength * (1.0 - d / SCOOP_RANGE) * dt) / d;
            b.vx += (s.x - b.x) * k;
            b.vy += (s.y - b.y) * k;
            d
        };
        for i in 0..self.items.len() {
            let it = &mut self.items[i];
            if it.dead || it.towed || it.hooked_by.is_some() || it.b.mass > SCOOP_MAX_MASS {
                continue;
            }
            let d = pull(&mut it.b, SCOOP_PULL);
            let (r, mass) = (it.b.r, it.b.mass);
            if d < s.r + r + 40.0 && self.tow_mass() + mass <= self.mods.winch_cap {
                self.attach_to_line(i);
                let (x, y) = (self.items[i].b.x, self.items[i].b.y);
                self.popup(x, y - 16.0, "SCOOPED".to_owned(), Tint::Cable);
                self.sfx(Sfx::Hook);
                self.radio(Radio::Scooped, false, true);
            }
        }
        for r in &mut self.rocks {
            if r.size == 1 && !r.dead {
                pull(&mut r.b, SCOOP_PEBBLE_PULL);
            }
        }
    }
}
