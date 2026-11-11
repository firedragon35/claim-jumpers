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
    /// Items-vector index of each link on pilot `p`'s chain, tug outward.
    pub(crate) fn chain_indices(&self, p: usize) -> Vec<usize> {
        self.pilots[p]
            .chain
            .iter()
            .filter_map(|id| self.item_index(*id))
            .collect()
    }

    /// The anchor body of link `link` on pilot `p`'s chain: the tug, or the
    /// previous item.
    pub(crate) fn anchor_body(&self, p: usize, link: usize) -> Body {
        if link == 0 {
            self.pilots[p].ship.b
        } else {
            let id = self.pilots[p].chain[link - 1];
            self.items[self.item_index(id).expect("chain item exists")].b
        }
    }

    /// Shortest a link can reel to: the two bodies plus a little slack.
    fn min_link_len(item: &Body, anchor: &Body) -> f64 {
        item.r + anchor.r + LINK_SLACK
    }

    /// Nearest free item within hook range of pilot `p`'s tug.
    pub fn find_hook_target(&self, p: usize) -> Option<usize> {
        let s = &self.pilots[p].ship;
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
            if d < self.pilots[p].mods.hook_range && d < best_d {
                best = Some(i);
                best_d = d;
            }
        }
        best
    }

    /// Attach the item nearest pilot `p`'s tug to the end of its chain.
    pub fn try_hook(&mut self, p: usize) {
        let Some(i) = self.find_hook_target(p) else {
            self.sfx(Sfx::Deny);
            return;
        };
        if self.tow_mass(p) + self.items[i].b.mass > self.pilots[p].mods.winch_cap {
            self.sfx(Sfx::Deny);
            self.radio(Radio::Maxed, true, false);
            return;
        }
        self.attach_to_line(p, i);
        self.sfx(Sfx::Hook);
        let kind = self.items[i].kind;
        if kind == crate::items::ItemType::Reactor {
            self.radio(Radio::Reactor, true, false);
        } else if kind == crate::items::ItemType::Engine {
            self.radio(Radio::Engine, false, false);
        } else if self.tow_mass(p) / self.pilots[p].mods.winch_cap > 0.6 {
            self.radio(Radio::Heavy, false, false);
        } else {
            self.radio(Radio::Hook, false, false);
        }
    }

    /// Append item `i` to the end of pilot `p`'s tow chain. The link starts
    /// at its current length; the winch reels it in over time, so a distant
    /// grab does not snap the cargo to the tug instantly.
    pub fn attach_to_line(&mut self, p: usize, i: usize) {
        let anchor = self.anchor_body(p, self.pilots[p].chain.len());
        let it = &mut self.items[i];
        it.towed = true;
        it.owner = Some(p);
        it.thrown_t = 0.0;
        it.link_len = Self::min_link_len(&it.b, &anchor).max(it.b.dist_to(&anchor));
        self.pilots[p].chain.push(it.id);
    }

    /// Remove one item from whichever chain holds it and re-link whatever
    /// hung behind it to the item's own anchor, so the train stays connected.
    pub fn remove_from_chain(&mut self, id: u32) {
        let found = self
            .pilots
            .iter()
            .enumerate()
            .find_map(|(p, pl)| pl.chain.iter().position(|c| *c == id).map(|idx| (p, idx)));
        let Some((p, idx)) = found else {
            return;
        };
        self.pilots[p].chain.remove(idx);
        if let Some(i) = self.item_index(id) {
            self.items[i].towed = false;
        }
        if idx < self.pilots[p].chain.len() {
            let a = self.anchor_body(p, idx);
            let ni = self
                .item_index(self.pilots[p].chain[idx])
                .expect("chain item exists");
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

    /// Detach the link at `idx` on pilot `p`'s chain and everything behind
    /// it (a severed line).
    pub fn sever_at(&mut self, p: usize, idx: usize) {
        if idx >= self.pilots[p].chain.len() {
            return;
        }
        let lost: Vec<u32> = self.pilots[p].chain.drain(idx..).collect();
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

    /// Drop the last piece on pilot `p`'s chain. It keeps its swing
    /// momentum, so this is a throw.
    pub fn drop_last(&mut self, p: usize) {
        let Some(&id) = self.pilots[p].chain.last() else {
            return;
        };
        self.sever_at(p, self.pilots[p].chain.len() - 1);
        if let Some(i) = self.item_index(id) {
            let it = &mut self.items[i];
            it.b.vx *= 1.1;
            it.b.vy *= 1.1;
            it.thrown_t = 1.6;
        }
        self.sfx(Sfx::Drop);
    }

    /// Cut pilot `p`'s whole chain loose.
    pub fn cut_all(&mut self, p: usize) {
        if self.pilots[p].chain.is_empty() {
            return;
        }
        self.pilots[p].threw_cargo = true;
        for i in self.chain_indices(p) {
            self.items[i].thrown_t = 1.2;
        }
        self.sever_at(p, 0);
        self.sfx(Sfx::Drop);
        self.radio(Radio::Cut, false, false);
    }

    /// Winch reel-in, then several rope solver passes over every link of
    /// every pilot's chain.
    pub(crate) fn solve_cables(&mut self, dt: f64) {
        self.solve_cables_for(None, dt);
    }

    /// The rope solver for every cable, or with `only` set just that
    /// pilot's cable and the tether of a collector holding that tug (for a
    /// client predicting its own tug): rival lines, the co-op partner and
    /// the collectors' reeling are then left to the server.
    pub(crate) fn solve_cables_for(&mut self, only: Option<usize>, dt: f64) {
        let mine = |p: usize| only.is_none_or(|o| o == p);
        for it in &mut self.items {
            it.jerk = 0.0;
        }
        let links: Vec<Vec<usize>> = (0..self.pilots.len())
            .map(|p| {
                if mine(p) {
                    self.chain_indices(p)
                } else {
                    Vec::new()
                }
            })
            .collect();
        for (p, chain) in links.iter().enumerate() {
            for (k, &i) in chain.iter().enumerate() {
                let anchor = self.anchor_body(p, k);
                let it = &mut self.items[i];
                let min = Self::min_link_len(&it.b, &anchor);
                if it.link_len > min {
                    it.link_len = min.max(it.link_len - self.pilots[p].mods.reel * dt);
                }
            }
        }
        // Rival lines: each jumper reels its hooked piece in to a fixed slack.
        let mut rival_links = Vec::new();
        for j in (0..self.jumpers.len()).filter(|_| only.is_none()) {
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
        // The collector holding each tug's net, if any.
        let tethers: Vec<Option<usize>> = self
            .pilots
            .iter()
            .enumerate()
            .map(|(p, pl)| {
                pl.ship
                    .net
                    .filter(|_| mine(p))
                    .and_then(|n| self.collector_index(n.by))
                    .filter(|&c| !self.collectors[c].dead)
            })
            .collect();
        // Multiple passes let corrections propagate down a long train. With
        // one pilot the order is the browser's: tug links, rival lines, the
        // tether, then the co-op partner.
        for _ in 0..CABLE_ITERATIONS {
            for (p, chain) in links.iter().enumerate() {
                if !self.pilots[p].ship.alive {
                    continue;
                }
                for (k, &i) in chain.iter().enumerate() {
                    let jerk = if k == 0 {
                        let it = &mut self.items[i];
                        rope_constraint(&mut self.pilots[p].ship.b, &mut it.b, it.link_len)
                    } else {
                        let (a, it) = two_items(&mut self.items, chain[k - 1], i);
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
            for (p, tether) in tethers.iter().enumerate() {
                if let Some(c) = tether.filter(|_| self.pilots[p].ship.alive) {
                    let cl = &mut self.collectors[c];
                    rope_constraint(&mut cl.b, &mut self.pilots[p].ship.b, cl.tether);
                }
            }
            if only.is_none() {
                self.partner_constraint();
            }
        }
        for c in self.collectors.iter_mut().filter(|_| only.is_none()) {
            if c.state == crate::opposition::CollectorState::Tow && c.tether > 160.0 {
                c.tether = (c.tether - 90.0 * dt).max(160.0);
            }
        }
        // Smoothed tension drives the cable's colour; big jerks rattle the cargo.
        let mut rattle = false;
        for &i in links.iter().flatten() {
            let it = &mut self.items[i];
            it.tension += ((it.jerk / 30.0).min(1.0) - it.tension) * 0.2;
            rattle |= it.jerk > RATTLE_JERK;
        }
        if rattle {
            self.sfx(Sfx::Rattle);
        }
    }

    /// Salvage grinder (grindLast): turn the last piece on pilot `p`'s line
    /// into heavy slugs, two per tonne. Its sale value is gone. A reactor
    /// detonates in the grinder.
    pub fn grind_last(&mut self, p: usize) {
        let last = self.pilots[p]
            .chain
            .last()
            .and_then(|id| self.item_index(*id));
        let Some(i) = last.filter(|_| self.pilots[p].mods.grinder && self.pilots[p].ship.alive)
        else {
            self.sfx(Sfx::Deny);
            return;
        };
        if self.items[i].def().volatile {
            self.radio(Radio::GrindReactor, true, false);
            self.items[i].primed_by = crate::world::Blame::Pilot(p);
            self.detonate(i);
            return;
        }
        let slugs = (self.items[i].b.mass * 2.0).ceil() as u32;
        self.remove_item(i);
        let s = &mut self.pilots[p].ship;
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

    /// Magnetic scoop (updateScoop) for every pilot, in pilot order.
    pub(crate) fn update_scoop(&mut self, dt: f64) {
        for p in 0..self.pilots.len() {
            self.with_hearer(Some(p), |w| w.update_scoop_for(p, dt));
        }
    }

    /// Pilot `p`'s magnetic scoop: pulls light salvage toward the tug and
    /// hooks it on contact. The catch is that it pulls pebbles in too.
    fn update_scoop_for(&mut self, p: usize, dt: f64) {
        if !self.pilots[p].mods.scoop || !self.pilots[p].ship.alive {
            return;
        }
        let s = self.pilots[p].ship.b;
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
            if d < s.r + r + 40.0 && self.tow_mass(p) + mass <= self.pilots[p].mods.winch_cap {
                self.attach_to_line(p, i);
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
