//! Who may hurt whom between players, and each pilot's PvP state.
//!
//! The owner's rules (2026-11-10 and 2026-11-11): a PvP Enabled pilot can
//! hurt any tug, Protected ones included. Anyone else's doing never harms
//! another player's tug, with one exception: a pilot attacked by a PvP
//! pilot may hurt that attacker back for [`PVP_COMBAT_LOCK`] seconds after
//! each attack (their payback). PvP exists only on a server whose host
//! allows it ([`World::pvp_allowed`], off by default), and single player
//! never has it.
//!
//! A pilot switches PvP in the bay: Protected, then Arming for
//! [`PVP_ARM_TIME`], then Enabled. Turning it off also happens in the bay,
//! and not while a combat lock ([`PVP_COMBAT_LOCK`]) runs. Every pilot
//! starts a shift Protected, because each shift builds fresh pilots.
//!
//! Radiation clouds are the exception to all of this: they cook any tug
//! that passes through, whoever's reactor made them (owner's rule,
//! 2026-11-11).

use crate::consts::{BAY_REPAIR_RANGE, BAY_X, BAY_Y};
use crate::events::Tint;
use crate::js;
use crate::world::World;

/// Seconds between switching PvP on in the bay and being PvP Enabled.
pub const PVP_ARM_TIME: f64 = 10.0;

/// Seconds after a PvP hit during which the PvP pilots involved can't turn
/// PvP off, and during which a Protected victim may hurt their attacker back.
pub const PVP_COMBAT_LOCK: f64 = 30.0;

/// Wanted stars on a lawful belt for an attack that starts a fight with a
/// player, and for wrecking one (as many as killing a sheriff).
pub const PVP_ATTACK_STARS: u32 = 1;
pub const PVP_WRECK_STARS: u32 = 2;

/// Seconds that cargo spilled by a PvP wreck stays hookable only by its
/// owner and PvP players.
pub const PVP_SPILL_CLAIM: f64 = 20.0;

/// A pilot's PvP state.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Pvp {
    /// Only PvP pilots can hurt this tug, and it can hurt only a PvP pilot
    /// who attacked it in the last [`PVP_COMBAT_LOCK`] seconds.
    #[default]
    Protected,
    /// Switched on in the bay; Enabled when `t` seconds have run out.
    Arming { t: f64 },
    /// Able to hurt any tug, and open to damage from other PvP pilots and
    /// from anyone it attacked.
    Enabled,
}

impl World {
    /// Whether damage that `by` answers for may hurt pilot `target`'s tug.
    /// `None` is the environment (rivals, the law, hazards nobody set off).
    /// It is judged when the damage lands, so a reactor primed while PvP
    /// was on can't hurt anyone once its owner is Protected again.
    pub fn can_hurt_pilot(&self, by: Option<usize>, target: usize) -> bool {
        by.is_none_or(|a| {
            a == target || self.pilots[a].pvp == Pvp::Enabled || self.has_payback(a, target)
        })
    }

    /// Whether pilot `defender` may hurt `attacker` back: `attacker` hurt
    /// them as a PvP pilot within the last [`PVP_COMBAT_LOCK`] seconds.
    /// (A PvP defender can hurt anyone anyway; the record still marks the
    /// fight as running, for the law.)
    pub fn has_payback(&self, defender: usize, attacker: usize) -> bool {
        self.pilots[defender]
            .payback
            .iter()
            .any(|&(a, _)| a == attacker)
    }

    /// Records that `by` hurt pilot `target` (another player's tug).
    ///
    /// A hit by a PvP Enabled pilot is an attack: it combat-locks the
    /// attacker, and gives (or renews) the victim's payback against them for
    /// [`PVP_COMBAT_LOCK`] seconds. A PvP victim is combat-locked too. A hit
    /// by anyone else is self-defence under payback: it locks a PvP victim
    /// but renews nothing for the defender, so only the attacker's own
    /// attacks restart the clock.
    ///
    /// The law (owner, 2026-11-11): on a lawful belt an attack that starts a
    /// fight, one whose victim had no running payback against this attacker,
    /// gives the attacker a wanted star. The Frontier has no law, and
    /// self-defence never earns stars.
    pub(crate) fn record_pvp_hit(&mut self, by: usize, target: usize) {
        if by == target {
            return;
        }
        // Remembered for this step only: if the tug is wrecked now, `by`
        // dealt the blow (see `credit_pvp_wreck`).
        self.pilots[target].last_pvp_hit = Some((by, self.t));
        let attack = self.pilots[by].pvp == Pvp::Enabled;
        let victim = &mut self.pilots[target];
        if victim.pvp == Pvp::Enabled {
            victim.combat_t = PVP_COMBAT_LOCK;
        }
        if !attack {
            return;
        }
        self.pilots[by].combat_t = PVP_COMBAT_LOCK;
        let victim = &mut self.pilots[target];
        let first = !victim.payback.iter().any(|&(a, _)| a == by);
        victim.payback.retain(|&(a, _)| a != by);
        victim.payback.push((by, PVP_COMBAT_LOCK));
        if first {
            self.pvp_crime(by, target, PVP_ATTACK_STARS);
        }
    }

    /// Pilot `p`'s tug was just wrecked: if another player's hit landed in
    /// this same step, that player is credited with a PvP kill (counted in
    /// the player list) and, if it was an attack on a lawful belt, made
    /// more wanted. PvP kills pay no reward (owner, 2026-11-10); the victim
    /// pays the usual deductible. Returns whether it was a PvP wreck.
    pub(crate) fn credit_pvp_wreck(&mut self, p: usize) -> bool {
        let Some((by, at)) = self.pilots[p].last_pvp_hit.take() else {
            return false;
        };
        // The same step's time, copied exactly, so equality is the test;
        // an older hit followed by a rock or a rival credits nobody.
        #[allow(clippy::float_cmp)]
        if at != self.t {
            return false;
        }
        self.pilots[by].stats.pvp_kills += 1;
        if self.pilots[by].pvp == Pvp::Enabled {
            self.pvp_crime(by, p, PVP_WRECK_STARS);
        }
        true
    }

    /// Marks the cargo on pilot `p`'s chain, about to spill from a PvP
    /// wreck: for [`PVP_SPILL_CLAIM`] seconds only `p` and PvP players may
    /// hook it, then it is free salvage (owner, 2026-11-10).
    pub(crate) fn claim_spill(&mut self, p: usize) {
        let until = self.t + PVP_SPILL_CLAIM;
        for i in self.chain_indices(p) {
            self.items[i].claim = Some((p, until));
        }
    }

    /// Whether pilot `p` may hook `it`: anything but PvP-wreck cargo under
    /// someone else's claim, which only PvP players may take.
    pub(crate) fn may_hook(&self, p: usize, it: &crate::entities::Item) -> bool {
        match it.claim {
            Some((owner, until)) if self.t < until && owner != p => {
                self.pilots[p].pvp == Pvp::Enabled
            }
            _ => true,
        }
    }

    /// Wanted stars for attacker `by` against pilot `target`, unless the
    /// belt is lawless (the Frontier). The popup shows at the victim's tug.
    fn pvp_crime(&mut self, by: usize, target: usize, stars: u32) {
        if self.belt.def().lawless {
            return;
        }
        let b = self.pilots[target].ship.b;
        self.with_hearer(Some(by), |w| w.add_wanted(by, stars, b.x, b.y));
    }

    /// Pilot `p` asks to switch PvP (the toggle key). It only works with
    /// the tug alive in the bay on a server that allows PvP: Protected
    /// starts arming, arming is called off, and Enabled goes back to
    /// Protected unless a combat lock is running. A popup says what happened
    /// or why not.
    pub(crate) fn toggle_pvp(&mut self, p: usize) {
        let s = &self.pilots[p].ship;
        let (x, y) = (s.b.x, s.b.y - 40.0);
        if !self.pvp_allowed {
            self.popup(x, y, "PvP is off on this server".to_owned(), Tint::Muted);
            return;
        }
        if !s.alive {
            return;
        }
        if js::dist(s.b.x, s.b.y, BAY_X, BAY_Y) >= BAY_REPAIR_RANGE {
            self.popup(x, y, "Switch PvP in the bay".to_owned(), Tint::Muted);
            return;
        }
        let pl = &mut self.pilots[p];
        let (next, text, tint) = match pl.pvp {
            Pvp::Protected => (
                Pvp::Arming { t: PVP_ARM_TIME },
                format!("PvP arming: {PVP_ARM_TIME:.0} s"),
                Tint::Danger,
            ),
            Pvp::Arming { .. } => (Pvp::Protected, "PvP off".to_owned(), Tint::Muted),
            Pvp::Enabled if pl.combat_t > 0.0 => {
                let left = pl.combat_t.ceil();
                self.popup(x, y, format!("Combat lock: {left:.0} s"), Tint::Danger);
                return;
            }
            Pvp::Enabled => (Pvp::Protected, "PvP off".to_owned(), Tint::Muted),
        };
        pl.pvp = next;
        self.popup(x, y, text, tint);
    }

    /// Runs every pilot's arming countdown, combat lock and payback windows.
    /// They run while the tug is wrecked or its player is away, so none of
    /// them can be waited out by leaving.
    pub(crate) fn update_pvp(&mut self, dt: f64) {
        for p in 0..self.pilots.len() {
            let pl = &mut self.pilots[p];
            pl.combat_t = (pl.combat_t - dt).max(0.0);
            for (_, t) in &mut pl.payback {
                *t -= dt;
            }
            pl.payback.retain(|&(_, t)| t > 0.0);
            // A player who left mid-fight: their tug leaves once the lock
            // is over.
            if pl.leaving && pl.combat_t == 0.0 {
                self.set_away(p);
            }
            let pl = &mut self.pilots[p];
            let Pvp::Arming { t } = &mut pl.pvp else {
                continue;
            };
            *t -= dt;
            if *t <= 0.0 {
                pl.pvp = Pvp::Enabled;
                let b = pl.ship.b;
                self.with_hearer(Some(p), |w| {
                    w.popup(b.x, b.y - 40.0, "PVP ENABLED".to_owned(), Tint::Danger)
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::Action;
    use crate::mods::Mods;
    use crate::rng::Rng;
    use crate::world::{Pilot, World};

    /// A two-pilot field whose host allows PvP, both tugs in the bay.
    fn pvp_field() -> World {
        let mut w = World::empty(Rng::Constant(0.5), (1280.0, 720.0));
        w.pilots.push(Pilot::new(w.pilots[0].mods.clone()));
        w.pvp_allowed = true;
        w
    }

    /// Runs the PvP clocks for `secs` seconds in whole steps.
    fn run(w: &mut World, secs: f64) {
        for _ in 0..(secs * 60.0).round() as u32 {
            w.update_pvp(1.0 / 60.0);
        }
    }

    /// The environment and a pilot's own doing hurt; another pilot's doesn't.
    #[test]
    fn only_the_environment_or_yourself_can_hurt_you() {
        let w = pvp_field();
        assert!(w.can_hurt_pilot(None, 1));
        assert!(w.can_hurt_pilot(Some(1), 1));
        assert!(!w.can_hurt_pilot(Some(0), 1));
    }

    /// An Enabled pilot can hurt any tug; arming isn't enough to hurt
    /// anyone, and a Protected pilot can't hurt a PvP pilot unprovoked.
    #[test]
    fn a_pvp_pilot_can_hurt_any_tug() {
        let mut w = pvp_field();
        w.pilots[0].pvp = Pvp::Enabled;
        assert!(w.can_hurt_pilot(Some(0), 1), "the target is Protected");
        assert!(!w.can_hurt_pilot(Some(1), 0), "no attack, no payback");
        w.pilots[1].pvp = Pvp::Arming { t: 1.0 };
        assert!(!w.can_hurt_pilot(Some(1), 0), "arming can't hurt anyone");
        w.pilots[1].pvp = Pvp::Enabled;
        assert!(w.can_hurt_pilot(Some(1), 0));
    }

    /// A Protected pilot attacked by a PvP pilot may hurt that attacker,
    /// and only that one, for 30 s after the attacker's latest hit.
    #[test]
    fn the_victim_may_hurt_the_attacker_back_for_30_s() {
        let mut w = pvp_field();
        w.pilots.push(Pilot::new(w.pilots[0].mods.clone()));
        w.pilots[0].pvp = Pvp::Enabled;
        w.pilots[2].pvp = Pvp::Enabled;
        w.record_pvp_hit(0, 1);
        assert!(w.can_hurt_pilot(Some(1), 0), "payback on the attacker");
        assert!(!w.can_hurt_pilot(Some(1), 2), "not on a bystander");
        assert_eq!(w.pilots[0].combat_t, PVP_COMBAT_LOCK, "attacker locked");
        assert_eq!(w.pilots[1].combat_t, 0.0, "the victim isn't locked");
        // Self-defence doesn't renew the victim's payback...
        run(&mut w, 20.0);
        w.record_pvp_hit(1, 0);
        // Half a second past 30 s, clear of rounding in the summed steps.
        run(&mut w, 10.5);
        assert!(!w.can_hurt_pilot(Some(1), 0), "payback ran out");
        // ...while the attacker's next hit does.
        w.record_pvp_hit(0, 1);
        run(&mut w, 29.0);
        assert!(w.can_hurt_pilot(Some(1), 0));
    }

    /// A hit between two PvP pilots locks both of them.
    #[test]
    fn a_hit_between_pvp_pilots_locks_both() {
        let mut w = pvp_field();
        w.pilots[0].pvp = Pvp::Enabled;
        w.pilots[1].pvp = Pvp::Enabled;
        w.record_pvp_hit(0, 1);
        assert_eq!(w.pilots[0].combat_t, PVP_COMBAT_LOCK);
        assert_eq!(w.pilots[1].combat_t, PVP_COMBAT_LOCK);
    }

    /// `pvp_field` on `belt`, with pilot 0 PvP Enabled and pilot 1 one hit
    /// from wrecked.
    fn fight_on(belt: crate::field::Belt) -> World {
        let mut w = pvp_field();
        w.belt = belt;
        w.pilots[0].pvp = Pvp::Enabled;
        let s = &mut w.pilots[1].ship;
        (s.invuln, s.shield, s.hp) = (0.0, 0.0, 1.0);
        w
    }

    /// On a lawful belt the hit that starts a fight costs the attacker a
    /// star, more hits in the same fight cost nothing, and the wreck two.
    #[test]
    fn a_lawful_belt_stars_the_first_hit_and_the_wreck() {
        let mut w = fight_on(crate::field::Belt::Hollis);
        w.record_pvp_hit(0, 1);
        assert_eq!(w.pilots[0].wanted, 1);
        run(&mut w, 5.0);
        w.record_pvp_hit(0, 1);
        assert_eq!(w.pilots[0].wanted, 1, "the same fight");
        w.damage_ship(1, 5.0);
        assert_eq!(w.pilots[0].wanted, 1 + PVP_WRECK_STARS);
        assert_eq!(w.pilots[1].wanted, 0);
    }

    /// Once the fight's payback has run out, the next attack starts a new
    /// fight and costs another star.
    #[test]
    fn a_new_fight_costs_another_star() {
        let mut w = fight_on(crate::field::Belt::Hollis);
        w.record_pvp_hit(0, 1);
        run(&mut w, PVP_COMBAT_LOCK + 0.5);
        w.record_pvp_hit(0, 1);
        assert_eq!(w.pilots[0].wanted, 2);
    }

    /// The Frontier has no law: attacks and wrecks cost no stars.
    #[test]
    fn the_frontier_has_no_law() {
        let mut w = fight_on(crate::field::Belt::Frontier);
        w.record_pvp_hit(0, 1);
        w.damage_ship(1, 5.0);
        assert!(!w.pilots[1].ship.alive);
        assert_eq!(w.pilots[0].wanted, 0);
    }

    /// Firing back under payback, even wrecking the attacker, is
    /// self-defence: no stars, though the kill still counts.
    #[test]
    fn self_defence_earns_no_stars() {
        let mut w = fight_on(crate::field::Belt::Hollis);
        w.record_pvp_hit(0, 1);
        let s = &mut w.pilots[0].ship;
        (s.invuln, s.shield, s.hp) = (0.0, 0.0, 1.0);
        w.record_pvp_hit(1, 0);
        w.damage_ship(0, 5.0);
        assert!(!w.pilots[0].ship.alive);
        assert_eq!(w.pilots[1].wanted, 0);
        assert_eq!(w.pilots[1].stats.pvp_kills, 1);
    }

    /// Switching on in the bay arms for 10 s, then the pilot is Enabled.
    #[test]
    fn arming_takes_ten_seconds() {
        let mut w = pvp_field();
        w.action_for(0, Action::TogglePvp);
        assert!(matches!(w.pilots[0].pvp, Pvp::Arming { .. }));
        run(&mut w, PVP_ARM_TIME - 0.5);
        assert!(matches!(w.pilots[0].pvp, Pvp::Arming { .. }));
        run(&mut w, 0.5);
        assert_eq!(w.pilots[0].pvp, Pvp::Enabled);
        assert_eq!(w.pilots[1].pvp, Pvp::Protected, "only the pilot who asked");
    }

    /// Pressing again while arming calls it off.
    #[test]
    fn arming_can_be_called_off() {
        let mut w = pvp_field();
        w.action_for(0, Action::TogglePvp);
        run(&mut w, 3.0);
        w.action_for(0, Action::TogglePvp);
        assert_eq!(w.pilots[0].pvp, Pvp::Protected);
        run(&mut w, PVP_ARM_TIME);
        assert_eq!(w.pilots[0].pvp, Pvp::Protected);
    }

    /// The toggle works only in the bay, both ways.
    #[test]
    fn the_toggle_works_only_in_the_bay() {
        let mut w = pvp_field();
        w.pilots[0].ship.b.x += BAY_REPAIR_RANGE + 10.0;
        w.action_for(0, Action::TogglePvp);
        assert_eq!(w.pilots[0].pvp, Pvp::Protected);
        w.pilots[0].pvp = Pvp::Enabled;
        w.action_for(0, Action::TogglePvp);
        assert_eq!(w.pilots[0].pvp, Pvp::Enabled, "can't turn it off outside");
    }

    /// A combat lock keeps PvP on until it runs out, even while wrecked.
    #[test]
    fn the_combat_lock_holds_pvp_on() {
        let mut w = pvp_field();
        w.pilots[0].pvp = Pvp::Enabled;
        w.pilots[0].combat_t = PVP_COMBAT_LOCK;
        w.action_for(0, Action::TogglePvp);
        assert_eq!(w.pilots[0].pvp, Pvp::Enabled);
        w.pilots[0].ship.alive = false;
        run(&mut w, PVP_COMBAT_LOCK + 0.5);
        assert_eq!(w.pilots[0].combat_t, 0.0);
        w.pilots[0].ship.alive = true;
        w.action_for(0, Action::TogglePvp);
        assert_eq!(w.pilots[0].pvp, Pvp::Protected);
    }

    /// Pilot 1's tug overlapping pilot 0's, the two closing at 800 px/s,
    /// with their spawn grace over; then one collision pass.
    fn ram(pvp: [Pvp; 2]) -> World {
        let mut w = pvp_field();
        for (p, state) in pvp.into_iter().enumerate() {
            w.pilots[p].pvp = state;
            w.pilots[p].ship.invuln = 0.0;
        }
        let a = w.pilots[0].ship.b;
        let b = &mut w.pilots[1].ship.b;
        b.x = a.x + a.r + b.r - 4.0;
        b.y = a.y;
        b.vx = -400.0;
        w.pilots[0].ship.b.vx = 400.0;
        w.resolve_collisions();
        w
    }

    /// Two PvP tugs that ram each other are both hurt and both locked.
    #[test]
    fn ramming_hurts_between_two_pvp_tugs() {
        let fresh = Pilot::new(Mods::stock()).ship;
        let w = ram([Pvp::Enabled, Pvp::Enabled]);
        for p in 0..2 {
            let s = &w.pilots[p].ship;
            assert!(s.shield + s.hp < fresh.shield + fresh.hp, "pilot {p}");
            assert_eq!(w.pilots[p].combat_t, PVP_COMBAT_LOCK);
        }
    }

    /// Tugs still push each other, but contact hurts only between two PvP
    /// tugs, whatever one of them may otherwise hurt.
    #[test]
    fn ramming_a_protected_tug_hurts_nobody() {
        let fresh = Pilot::new(Mods::stock()).ship;
        let w = ram([Pvp::Enabled, Pvp::Protected]);
        for p in 0..2 {
            let s = &w.pilots[p].ship;
            assert_eq!((s.shield, s.hp), (fresh.shield, fresh.hp), "pilot {p}");
        }
        assert!(w.pilots[1].ship.b.vx > -400.0, "they still pushed");
    }

    /// Wrecking another player's tug is credited, and pays nothing.
    #[test]
    fn a_pvp_wreck_is_credited_to_the_attacker() {
        let mut w = pvp_field();
        w.pilots[0].pvp = Pvp::Enabled;
        let banked = w.pilots[0].banked;
        let s = &mut w.pilots[1].ship;
        (s.invuln, s.shield, s.hp) = (0.0, 0.0, 1.0);
        w.record_pvp_hit(0, 1);
        w.damage_ship(1, 5.0);
        assert!(!w.pilots[1].ship.alive);
        assert_eq!(w.pilots[0].stats.pvp_kills, 1);
        assert_eq!(w.pilots[0].banked, banked, "no reward");
    }

    /// A hit from an earlier step followed by the environment's credits
    /// nobody.
    #[test]
    fn an_old_hit_gets_no_credit_for_a_later_wreck() {
        let mut w = pvp_field();
        w.pilots[0].pvp = Pvp::Enabled;
        let s = &mut w.pilots[1].ship;
        (s.invuln, s.shield, s.hp) = (0.0, 0.0, 1.0);
        w.record_pvp_hit(0, 1);
        w.t += 1.0 / 60.0;
        w.damage_ship(1, 5.0);
        assert!(!w.pilots[1].ship.alive);
        assert_eq!(w.pilots[0].stats.pvp_kills, 0);
    }

    /// Cargo spilled by a PvP wreck: for 20 s its owner and PvP players
    /// may hook it but a Protected bystander can't; then anyone can. Cargo
    /// from a wreck nobody caused is free at once.
    #[test]
    fn pvp_wreck_cargo_is_held_for_20_s() {
        let mut w = fight_on(crate::field::Belt::Frontier);
        w.pilots.push(Pilot::new(w.pilots[0].mods.clone()));
        let b = w.pilots[1].ship.b;
        let it = w.new_item(crate::items::ItemType::Scrap, b.x + 40.0, b.y, 0.0, 0.0);
        w.items.push(it);
        w.attach_to_line(1, w.items.len() - 1);
        w.record_pvp_hit(0, 1);
        w.damage_ship(1, 5.0);
        assert!(!w.pilots[1].ship.alive);
        let spilled = &w.items[w.items.len() - 1];
        assert!(!spilled.towed);
        // Both other tugs right beside the spilled piece.
        for p in [0, 2] {
            w.pilots[p].ship.b.x = spilled.b.x - 30.0;
            w.pilots[p].ship.b.y = spilled.b.y;
        }
        assert!(w.find_hook_target(2).is_none(), "the bystander is held off");
        assert!(w.find_hook_target(0).is_some(), "a PvP player may take it");
        assert!(w.may_hook(1, &w.items[w.items.len() - 1]), "its owner may");
        w.t += PVP_SPILL_CLAIM;
        assert!(w.find_hook_target(2).is_some(), "free salvage after 20 s");
    }

    /// A player who leaves during a combat lock leaves their tug in the
    /// field until the lock runs out; without a lock it leaves at once.
    #[test]
    fn a_locked_tug_stays_until_the_lock_runs_out() {
        let mut w = pvp_field();
        w.pilots[0].pvp = Pvp::Enabled;
        w.pilots[1].pvp = Pvp::Enabled;
        w.record_pvp_hit(0, 1);
        w.set_away(1);
        assert!(w.pilots[1].away.is_none() && w.pilots[1].ship.alive);
        run(&mut w, PVP_COMBAT_LOCK - 1.0);
        assert!(w.pilots[1].away.is_none(), "still in the fight");
        run(&mut w, 1.5);
        assert!(w.pilots[1].away.is_some() && !w.pilots[1].ship.alive);
        w.set_away(0);
        assert!(w.pilots[0].away.is_some(), "no lock left: away at once");
    }

    /// Leaving and coming back doesn't reset or end the lock: the tug never
    /// left, and still can't switch PvP off.
    #[test]
    fn reconnecting_doesnt_reset_the_lock() {
        let mut w = pvp_field();
        w.pilots[0].pvp = Pvp::Enabled;
        w.pilots[1].pvp = Pvp::Enabled;
        w.record_pvp_hit(0, 1);
        run(&mut w, 10.0);
        w.set_away(1);
        w.set_back(1);
        let left = w.pilots[1].combat_t;
        assert!((left - (PVP_COMBAT_LOCK - 10.0)).abs() < 1e-6);
        w.action_for(1, Action::TogglePvp);
        assert_eq!(w.pilots[1].pvp, Pvp::Enabled);
        run(&mut w, PVP_COMBAT_LOCK);
        assert!(w.pilots[1].away.is_none(), "returned, so it stays");
    }

    /// Without the host's say-so, and in single player, the toggle does
    /// nothing.
    #[test]
    fn no_pvp_unless_the_host_allows_it() {
        let mut w = pvp_field();
        w.pvp_allowed = false;
        w.action_for(0, Action::TogglePvp);
        run(&mut w, PVP_ARM_TIME + 1.0);
        assert_eq!(w.pilots[0].pvp, Pvp::Protected);
        let solo = World::new_shift(Rng::Constant(0.5), (1280.0, 720.0));
        assert!(!solo.pvp_allowed);
    }
}
