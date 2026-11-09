//! Cannon rounds, rival bolts, cutting wrecks open, hurting rivals and
//! reactor blasts (updateBullets, damageWreck, ejectFromWreck, breakWreck,
//! hurtJumper, primeReactor, detonate and explode in
//! src/js/16-simulation-step.js).
//!
//! The browser loops over arrays that can grow while they are walked (a
//! blast breaks a rock, and the rock's pieces are visited by the same
//! loop). The index loops here re-read the length each pass for the same
//! reason, so those late arrivals are treated the same way.

use crate::consts::Variant;
use crate::consts::*;
use crate::entities::{make_crab, make_rock, Body, Bullet, CrabState};
use crate::events::{Event, Radio, Sfx, Tint};
use crate::items::ItemType;
use crate::js;
use crate::world::{fmt_credits, World};
use std::f64::consts::TAU;

/// `hits(b, o, pad)`: a round at (x, y) is inside body `o` grown by `pad`.
/// The cheap axis test comes first, as in the browser.
fn hits(x: f64, y: f64, o: &Body, pad: f64) -> bool {
    let r = o.r + pad;
    (x - o.x).abs() < r && js::dist(x, y, o.x, o.y) < r
}

impl World {
    /// Advances every bullet one step: movement, lifetime and what it hits.
    pub(crate) fn update_bullets(&mut self, dt: f64) {
        let mut bullets = std::mem::take(&mut self.bullets);
        for b in &mut bullets {
            b.x += b.vx * dt;
            b.y += b.vy * dt;
            b.life -= dt;
            if b.life <= 0.0 {
                continue;
            }
            if self.bullet_hit(b) {
                b.life = 0.0;
                self.emit(Event::Hit { x: b.x, y: b.y });
            }
        }
        bullets.retain(|b| b.life > 0.0);
        // Nothing fires the cannon during this pass, but keep any that did.
        bullets.append(&mut self.bullets);
        self.bullets = bullets;

        let mut ebullets = std::mem::take(&mut self.ebullets);
        for b in &mut ebullets {
            b.x += b.vx * dt;
            b.y += b.vy * dt;
            b.life -= dt;
            let s = &self.ship;
            if s.alive && b.life > 0.0 && js::dist(b.x, b.y, s.b.x, s.b.y) < s.b.r + 3.0 {
                let open = s.invuln <= 0.0;
                if open {
                    self.sfx(Sfx::ZapHit);
                    self.under_attack();
                }
                // Sheriff stun bolts hit softer but leave the tug sluggish.
                if b.stun && open {
                    self.ship.stun_t = crate::opposition::STUN_TIME;
                }
                self.damage_ship(b.dmg);
                b.life = 0.0;
                let tint = if b.swat || b.rogue {
                    Tint::Danger
                } else if b.stun {
                    Tint::Law
                } else {
                    Tint::Jumper
                };
                self.burst(b.x, b.y, 5, tint, 100.0);
            }
            // A rogue's bolt hits whatever crosses it.
            if b.rogue && b.life > 0.0 {
                let hit = self.rogue_bolt_target(b.x, b.y);
                if let Some(t) = hit {
                    self.rogue_bolt_hit(t);
                    b.life = 0.0;
                    self.burst(b.x, b.y, 5, Tint::Danger, 100.0);
                }
            }
            // Enemy fire can knock out a sentry in the way.
            if b.life > 0.0 {
                let hit = (0..self.sentries.len()).find(|&i| {
                    let d = &self.sentries[i];
                    !d.dead && !(b.rogue && d.rogue) && js::dist(b.x, b.y, d.x, d.y) <= d.r + 3.0
                });
                if let Some(i) = hit {
                    self.hurt_sentry(i, if b.stun { 2.0 } else { 3.0 });
                    b.life = 0.0;
                    self.burst(b.x, b.y, 5, Tint::Cable, 100.0);
                }
            }
            // Enemy fire that reaches an escorted hauler damages it.
            if b.life > 0.0 {
                let hit = self
                    .hauler
                    .as_ref()
                    .is_some_and(|h| !h.dead && js::dist(b.x, b.y, h.b.x, h.b.y) < h.b.r + 3.0);
                if hit {
                    // Sheriff and rogue bolts carry their damage; rival
                    // and drone bolts do 4 to a hauler (5 to the tug).
                    let n = if b.stun || b.swat || b.rogue {
                        b.dmg
                    } else {
                        4.0
                    };
                    self.hurt_hauler(n);
                    b.life = 0.0;
                    self.burst(b.x, b.y, 5, Tint::Steel, 100.0);
                }
            }
            // Enemy fire can set off volatile cargo, towed or not.
            if b.life > 0.0 {
                for i in 0..self.items.len() {
                    let it = &self.items[i];
                    if it.dead
                        || !it.def().volatile
                        || js::dist(b.x, b.y, it.b.x, it.b.y) > it.b.r + 3.0
                    {
                        continue;
                    }
                    b.life = 0.0;
                    self.burst(b.x, b.y, 5, Tint::Danger, 100.0);
                    if self.item_hit(i, 1.0) {
                        self.prime_reactor(i);
                    }
                    break;
                }
            }
        }
        ebullets.retain(|b| b.life > 0.0);
        ebullets.append(&mut self.ebullets);
        self.ebullets = ebullets;
    }

    /// Resolve one of your rounds against the field, in the browser's
    /// target order. Returns true if it struck something.
    fn bullet_hit(&mut self, b: &Bullet) -> bool {
        let dmg = b.dmg;
        if let Some(i) = (0..self.rocks.len())
            .find(|&i| !self.rocks[i].dead && hits(b.x, b.y, &self.rocks[i].b, 0.0))
        {
            self.damage_rock(i, dmg);
            return true;
        }
        if let Some(i) = (0..self.wrecks.len())
            .find(|&i| !self.wrecks[i].dead && hits(b.x, b.y, &self.wrecks[i].b, 10.0))
        {
            self.damage_wreck(i, dmg);
            return true;
        }
        if let Some(i) = (0..self.jumpers.len())
            .find(|&i| !self.jumpers[i].dead && hits(b.x, b.y, &self.jumpers[i].b, 4.0))
        {
            self.hurt_jumper(i, dmg, None);
            return true;
        }
        if let Some(i) = (0..self.drones.len())
            .find(|&i| !self.drones[i].dead && hits(b.x, b.y, &self.drones[i].b, 4.0))
        {
            self.hurt_drone(i, dmg);
            return true;
        }
        if let Some(i) = (0..self.miners.len())
            .find(|&i| !self.miners[i].dead && hits(b.x, b.y, &self.miners[i].b, 4.0))
        {
            self.hurt_miner(i, dmg, true);
            return true;
        }
        if !b.sentry {
            if let Some(i) = (0..self.sentries.len()).find(|&i| {
                let d = &self.sentries[i];
                let body = Body {
                    x: d.x,
                    y: d.y,
                    r: d.r,
                    ..Body::default()
                };
                d.rogue && !d.dead && hits(b.x, b.y, &body, 4.0)
            }) {
                self.hurt_sentry(i, dmg);
                return true;
            }
            // Sentry fire never touches the law; your own cannon still can.
            if let Some(k) = (0..self.sheriffs.len())
                .find(|&k| !self.sheriffs[k].dead && hits(b.x, b.y, &self.sheriffs[k].b, 4.0))
            {
                self.sheriffs[k].provoked = true;
                self.hurt_sheriff(k, dmg, None);
                return true;
            }
        }
        if let Some(c) = (0..self.collectors.len())
            .find(|&c| !self.collectors[c].dead && hits(b.x, b.y, &self.collectors[c].b, 4.0))
        {
            if !b.sentry {
                self.collectors[c].provoked = true; // your cannon, not a stray bolt
            }
            let n = if b.slug {
                dmg
            } else {
                dmg * crate::opposition::COLLECTOR_BULLET_ARMOR
            };
            self.hurt_collector(c, n, None);
            if !b.slug {
                self.burst(b.x, b.y, 3, Tint::Text, 120.0); // sparks off the armor
            }
            return true;
        }
        // Shots into the awake maw damage the jaws; the hub stops shots.
        if let Some(r) = &self.refinery {
            let (mx, my) = r.maw();
            if r.jaws_vulnerable() && js::dist(b.x, b.y, mx, my) < crate::opposition::MAW_R {
                self.hurt_jaws(dmg);
                self.burst(b.x, b.y, 4, Tint::Danger, 100.0);
                return true;
            }
            if hits(b.x, b.y, &r.b, 0.0) {
                return true;
            }
        }
        if let Some(c) = (0..self.crabs.len()).find(|&c| {
            let cr = &self.crabs[c];
            let body = Body {
                x: cr.x,
                y: cr.y,
                r: cr.r,
                ..Body::default()
            };
            !cr.dead && cr.state != CrabState::Latched && hits(b.x, b.y, &body, 6.0)
        }) {
            self.kill_crab(c);
            return true;
        }
        if let Some(i) = (0..self.items.len())
            .find(|&i| !self.items[i].dead && hits(b.x, b.y, &self.items[i].b, 0.0))
        {
            // Shooting cargo nudges it; shooting a reactor is a very bad idea.
            let it = &mut self.items[i];
            it.b.vx += b.vx * 0.02 / it.b.mass;
            it.b.vy += b.vy * 0.02 / it.b.mass;
            if it.def().volatile && self.item_hit(i, dmg) {
                self.prime_reactor(i);
            }
            return true;
        }
        (0..CARRIER_HULL.len()).any(|k| {
            let (dx, r) = CARRIER_HULL[k];
            let hull = Body {
                x: CARRIER_X + dx,
                y: CARRIER_Y,
                r,
                ..Body::default()
            };
            hits(b.x, b.y, &hull, 0.0)
        })
    }

    /// Take `n` off an item's hit points; true once they reach zero.
    /// Indestructible items (no hit points) never break.
    fn item_hit(&mut self, i: usize, n: f64) -> bool {
        match self.items[i].hp.as_mut() {
            Some(hp) => {
                *hp -= n;
                *hp <= 0.0
            }
            None => false,
        }
    }

    /// Sparks flying out from a point.
    pub(crate) fn burst(&mut self, x: f64, y: f64, n: u32, tint: Tint, speed: f64) {
        self.emit(Event::Burst {
            x,
            y,
            n,
            tint,
            speed,
        });
    }

    /// Cut into a wreck. A piece of its contents comes out each time the
    /// hull crosses an evenly spaced threshold; the last piece waits for
    /// the hull to break.
    pub fn damage_wreck(&mut self, i: usize, n: f64) {
        let w = &mut self.wrecks[i];
        if w.dead {
            return;
        }
        // Only its keycard opens a sealed hold.
        if w.locked {
            self.sealed_hold_hit(i);
            return;
        }
        let before = w.hp;
        w.hp -= n;
        w.flash = 0.08;
        let per = w.max_hp / (w.contents.len() as f64 + 1.0);
        let crossed = ((w.max_hp - w.hp) / per).floor() - ((w.max_hp - before) / per).floor();
        self.sfx(Sfx::Chip);
        let mut k = 0.0;
        while k < crossed && self.wrecks[i].contents.len() > 1 {
            self.eject_from_wreck(i);
            k += 1.0;
        }
        if self.wrecks[i].hp <= 0.0 {
            self.break_wreck(i);
        }
    }

    /// Throw the next piece out of a wreck. Crabs hiding inside stow away
    /// on the pieces that come out.
    fn eject_from_wreck(&mut self, i: usize) {
        let Some(kind) = self.wrecks[i].contents.pop() else {
            return;
        };
        let w = self.wrecks[i].b;
        let a = self.rng.range(0.0, TAU);
        let sp = self.rng.range(60.0, 120.0);
        let it = self.new_item(
            kind,
            w.x + a.cos() * w.r * 0.5,
            w.y + a.sin() * w.r * 0.4,
            w.vx + a.cos() * sp,
            w.vy + a.sin() * sp,
        );
        let (id, x, y) = (it.id, it.b.x, it.b.y);
        self.items.push(it);
        if self.wrecks[i].crabs > 0 && self.rng.next() < 0.5 {
            self.wrecks[i].crabs -= 1;
            let c = make_crab(&mut self.rng, CrabState::Ride, Some(id), x, y);
            self.crabs.push(c);
        }
        self.burst(x, y, 8, Tint::Wreck, 140.0);
    }

    /// The hull gives way: everything left spills out, its crabs scatter,
    /// and the wreck leaves a few pebbles behind.
    pub(crate) fn break_wreck(&mut self, i: usize) {
        while !self.wrecks[i].contents.is_empty() {
            self.eject_from_wreck(i);
        }
        let w = self.wrecks[i].b;
        for _ in 0..self.wrecks[i].crabs {
            let x = w.x + self.rng.range(-40.0, 40.0);
            let y = w.y + self.rng.range(-30.0, 30.0);
            let mut c = make_crab(&mut self.rng, CrabState::Free, None, x, y);
            c.vx = self.rng.range(-60.0, 60.0);
            c.vy = self.rng.range(-60.0, 60.0);
            self.crabs.push(c);
        }
        for _ in 0..3 {
            let x = w.x + self.rng.range(-60.0, 60.0);
            let y = w.y + self.rng.range(-40.0, 40.0);
            let vx = self.rng.range(-80.0, 80.0);
            let vy = self.rng.range(-80.0, 80.0);
            let mut pebble = make_rock(&mut self.rng, 1, x, y, vx, vy, false);
            pebble.crumble_t = Some(self.rng.range(PEBBLE_LIFE[0], PEBBLE_LIFE[1]));
            self.rocks.push(pebble);
        }
        self.wrecks[i].dead = true;
        self.explosion_fx(w.x, w.y, 160.0, Tint::Wreck);
        self.shake = 12.0;
        self.sfx(Sfx::Boom);
    }

    /// Damage a rival. `by_item` is the mass of the cargo that hit it, if
    /// it was cargo: a hit from three tonnes or more counts as a smash,
    /// which scatters its wingmates.
    pub fn hurt_jumper(&mut self, j: usize, n: f64, by_item: Option<f64>) {
        let jm = &mut self.jumpers[j];
        if jm.dead {
            return;
        }
        jm.hp -= n;
        jm.flash = 0.1;
        if jm.hp > 0.0 {
            return;
        }
        jm.dead = true;
        let (x, y, vx, vy) = (jm.b.x, jm.b.y, jm.b.vx, jm.b.vy);
        let variant = jm.variant;
        let hook = jm.hook.take();
        let boss = jm.boss.clone();
        // Every rival you scrap deepens the crews' grudge; a new stage is
        // announced. (A rogue sentry's kills, which are not yours, and the
        // fixer crew's half-grudge arrive in milestones 4 and 3.)
        let before = self.rival_stage();
        // A rogue sentry's kill isn't yours: no tally, grudge, standing or
        // bounty.
        if !self.no_blame {
            self.stats.jumpers += 1;
            self.rep_delta.rivals += crate::career::REP_RIVAL_KILL;
            // The fixer smooths things over: half the grudge per kill.
            self.grudge += if self.crew == Some(crate::field::Crew::Fixer) {
                0.5
            } else {
                1.0
            };
            let k = &mut self.kills;
            match variant {
                _ if boss.is_some() => {}
                Variant::Skiff => k.skiff += 1,
                Variant::Cutter => k.cutter += 1,
                Variant::Brute => k.brute += 1,
                Variant::Gunship => k.gunship += 1,
            }
            // Rival bounty by hull type, paid into this shift's earnings.
            // Boss crews pay their own, bigger bounty below instead.
            let bounty = if boss.is_some() {
                0.0
            } else {
                variant.def().bounty
            };
            if bounty > 0.0 {
                self.banked += bounty;
                self.bounties += bounty;
                self.popup(
                    x,
                    y - 60.0,
                    format!("+{} BOUNTY", fmt_credits(bounty)),
                    Tint::Hazard,
                );
            }
        }
        let after = self.rival_stage();
        if after > before {
            let line = [
                Radio::RivalsAnnoyed,
                Radio::RivalsAnnoyed,
                Radio::RivalsHostile,
                Radio::RivalsVendetta,
            ][after];
            self.radio(line, true, false);
            self.popup(
                x,
                y - 44.0,
                format!("RIVALS: {}", RIVAL_STAGES[after].name.to_uppercase()),
                Tint::Jumper,
            );
        }
        if let Some(i) = hook.and_then(|id| self.item_index(id)) {
            self.items[i].hooked_by = None;
        }
        let smashed = by_item.is_some_and(|m| m >= 3.0);
        // Smashing one with cargo scares off its wingmates for a moment.
        // A boss crew pays its bounty straight into this shift's earnings,
        // and its fall scares the other crews back a stage.
        if let Some(b) = boss.as_ref().filter(|_| !self.no_blame) {
            self.banked += b.bounty;
            self.bounties += b.bounty;
            self.kills.boss += 1;
            self.popup(
                x,
                y - 60.0,
                format!("BOUNTY +{} CR", fmt_credits(b.bounty)),
                Tint::Hazard,
            );
            self.sfx(Sfx::Jackpot);
            self.grudge = crate::field::grudge_one_stage_down(self.grudge);
            self.rep_delta.rivals += crate::career::REP_BOSS_KILL_RIVALS;
            self.boss_down = Some(b.name);
            self.emit(Event::Unlock("boss"));
            self.emit(Event::HitStop(crate::consts::HITSTOP_BOSS));
            self.emit(Event::SlowMo(crate::consts::SLOWMO_BOSS));
            self.emit(Event::Highlight {
                score: 2500.0,
                text: format!("Took down {}", b.name),
            });
            let v = fmt_credits(b.bounty);
            let text = format!(
                "That was {}. {v} cr bounty, paid. Don't let it go to your head.",
                b.name
            );
            self.radio_say(
                Radio::BossDown,
                true,
                false,
                text,
                format!("Bounty paid. {v} cr."),
            );
        }
        if smashed && !self.no_blame {
            self.scatter_rivals(j);
        }
        // Their wreckage is salvage too: tow it home.
        for _ in 0..self.rng.int(1, 2) {
            let px = x + self.rng.range(-10.0, 10.0);
            let py = y + self.rng.range(-10.0, 10.0);
            let pvx = vx + self.rng.range(-60.0, 60.0);
            let pvy = vy + self.rng.range(-60.0, 60.0);
            let it = self.new_item(ItemType::Scrap, px, py, pvx, pvy);
            self.items.push(it);
        }
        if self.rng.next() < 0.3 {
            let it = self.new_item(ItemType::Plate, x, y, vx, vy);
            self.items.push(it);
        }
        self.explosion_fx(x, y, 85.0, Tint::Jumper);
        self.sfx(Sfx::Boom);
        if smashed {
            self.popup(x, y - 24.0, "SMASHED".to_owned(), Tint::Cable);
            self.radio(Radio::Smash, true, false);
            if !self.no_blame {
                self.emit(Event::Unlock("smash"));
                self.emit(Event::HitStop(crate::consts::HITSTOP_SMASH));
                self.emit(Event::Highlight {
                    score: 700.0,
                    text: "Smashed a rival with cargo".to_owned(),
                });
            }
        } else {
            self.radio(Radio::Kill, false, false);
        }
    }

    /// Reactors blow on a short fuse so chain reactions ripple instead of
    /// recursing.
    pub fn prime_reactor(&mut self, i: usize) {
        let it = &mut self.items[i];
        if !it.dead && it.fuse <= 0.0 {
            it.fuse = REACTOR_FUSE;
        }
    }

    /// Removes reactor `i` and explodes it where it was.
    pub(crate) fn detonate(&mut self, i: usize) {
        if self.items[i].dead {
            return;
        }
        let (x, y) = (self.items[i].b.x, self.items[i].b.y);
        self.remove_item(i);
        self.explode(x, y, REACTOR_BLAST_RADIUS, REACTOR_BLAST_DAMAGE);
        self.radio(Radio::Boom, true, false);
    }

    /// Area blast: breaks rocks, cuts wrecks, kills crabs and rivals,
    /// primes other volatile cargo and shoves everything outward.
    pub fn explode(&mut self, x: f64, y: f64, radius: f64, ship_damage: f64) {
        // 1 at the centre (or touching it), falling to 0 at the radius,
        // measured to the body's edge.
        let falloff = |b: &Body| {
            let d = js::dist(b.x, b.y, x, y) - b.r;
            if d < radius {
                1.0 - d.max(0.0) / radius
            } else {
                0.0
            }
        };
        let shove = |b: &mut Body, f: f64| {
            if b.fixed {
                return;
            }
            let a = (b.y - y).atan2(b.x - x);
            let mass = if b.mass == 0.0 { 1.0 } else { b.mass };
            let imp = BLAST_SHOVE * f / mass.sqrt();
            b.vx += a.cos() * imp;
            b.vy += a.sin() * imp;
        };
        let mut i = 0;
        while i < self.rocks.len() {
            let f = falloff(&self.rocks[i].b);
            if f > 0.0 {
                shove(&mut self.rocks[i].b, f);
                self.damage_rock(i, (8.0 * f).ceil());
            }
            i += 1;
        }
        let mut i = 0;
        while i < self.wrecks.len() {
            let f = falloff(&self.wrecks[i].b);
            if f > 0.0 {
                self.damage_wreck(i, (10.0 * f).ceil());
            }
            i += 1;
        }
        let mut i = 0;
        while i < self.jumpers.len() {
            let f = falloff(&self.jumpers[i].b);
            if f > 0.0 {
                shove(&mut self.jumpers[i].b, f);
                self.hurt_jumper(i, 6.0 * f, None);
            }
            i += 1;
        }
        let mut i = 0;
        while i < self.drones.len() {
            let f = falloff(&self.drones[i].b);
            if f > 0.0 {
                shove(&mut self.drones[i].b, f);
                self.hurt_drone(i, 6.0 * f);
            }
            i += 1;
        }
        let mut i = 0;
        while i < self.collectors.len() {
            let f = falloff(&self.collectors[i].b);
            if f > 0.0 {
                shove(&mut self.collectors[i].b, f);
                self.hurt_collector(i, 6.0 * f, None);
            }
            i += 1;
        }
        let jaws = self
            .refinery
            .as_ref()
            .filter(|r| r.jaws_vulnerable())
            .map(|r| r.jaw_body());
        if let Some(jb) = jaws {
            let f = falloff(&jb);
            if f > 0.0 {
                self.hurt_jaws(15.0 * f);
            }
        }
        let mut i = 0;
        while i < self.miners.len() {
            let f = falloff(&self.miners[i].b);
            if f > 0.0 {
                shove(&mut self.miners[i].b, f);
                self.hurt_miner(i, 6.0 * f, false);
            }
            i += 1;
        }
        let mut i = 0;
        while i < self.sheriffs.len() {
            let f = falloff(&self.sheriffs[i].b);
            if f > 0.0 {
                shove(&mut self.sheriffs[i].b, f);
                self.hurt_sheriff(i, 6.0 * f, None);
            }
            i += 1;
        }
        let mut i = 0;
        while i < self.sentries.len() {
            let d = &self.sentries[i];
            let body = Body {
                x: d.x,
                y: d.y,
                r: d.r,
                ..Body::default()
            };
            let f = falloff(&body);
            if f > 0.0 {
                self.hurt_sentry(i, 6.0 * f);
            }
            i += 1;
        }
        let mut c = 0;
        while c < self.crabs.len() {
            let cr = &self.crabs[c];
            let body = Body {
                x: cr.x,
                y: cr.y,
                r: cr.r,
                ..Body::default()
            };
            if falloff(&body) > 0.0 {
                self.kill_crab(c);
            }
            c += 1;
        }
        let mut i = 0;
        while i < self.items.len() {
            let f = falloff(&self.items[i].b);
            if f > 0.0 {
                shove(&mut self.items[i].b, f);
                if self.items[i].def().volatile {
                    self.prime_reactor(i);
                }
            }
            i += 1;
        }
        if self.ship.alive {
            let f = falloff(&self.ship.b);
            if f > 0.0 {
                shove(&mut self.ship.b, f);
                self.damage_ship(ship_damage * f);
            }
        }
        self.explosion_fx(x, y, radius, Tint::Reactor);
        self.shake = 22.0;
        self.sfx(Sfx::Boom);
        self.emit(Event::Rumble {
            strength: 1.0,
            ms: 400.0,
        });
    }
}
