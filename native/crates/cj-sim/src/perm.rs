//! Who may hurt whom between players (CJN-14).
//!
//! The owner's rules of 2026-11-10: a player's doing never harms another
//! player's tug unless both are PvP Enabled, and PvP does not exist yet
//! (CJN-21). So for now a pilot can only be hurt by the environment or by
//! their own doing (their own reactor, their own swinging cargo).
//!
//! Radiation clouds are the exception: they cook any tug that passes
//! through, whoever's reactor made them (owner's rule, 2026-11-11).

use crate::world::World;

impl World {
    /// Whether damage that `by` answers for may hurt pilot `target`'s tug.
    /// `None` is the environment (rivals, the law, hazards nobody set off).
    pub fn can_hurt_pilot(&self, by: Option<usize>, target: usize) -> bool {
        by.is_none_or(|a| a == target)
    }
}

#[cfg(test)]
mod tests {
    use crate::rng::Rng;
    use crate::world::{Pilot, World};

    /// The environment and a pilot's own doing hurt; another pilot's doesn't.
    #[test]
    fn only_the_environment_or_yourself_can_hurt_you() {
        let mut w = World::empty(Rng::Constant(0.5), (1280.0, 720.0));
        let second = Pilot::new(w.pilots[0].mods.clone());
        w.pilots.push(second);
        assert!(w.can_hurt_pilot(None, 1));
        assert!(w.can_hurt_pilot(Some(1), 1));
        assert!(!w.can_hurt_pilot(Some(0), 1));
    }
}
