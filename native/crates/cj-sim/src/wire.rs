//! How simulation types travel on the network (cj-net), where that differs
//! from a plain derive: boss crew names go as text and are read back from
//! `field::BOSS_NAMES` (so a message can't introduce an unknown name), and
//! body coordinates go as f32 (see [`f32_wire`]).

/// `#[serde(with = ...)]` sending an f64 as an f32. The simulation runs on
/// f64; across the 5,200-unit field an f32 keeps about 0.0005 units, far
/// below anything drawn or felt, at half the bytes.
pub mod f32_wire {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &f64, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_f32(*v as f32)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
        f32::deserialize(d).map(f64::from)
    }
}

/// A boss as it travels: the name as owned text.
#[derive(serde::Serialize, serde::Deserialize)]
struct BossWire {
    name: String,
    bounty: f64,
}

impl serde::Serialize for crate::entities::Boss {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        BossWire {
            name: self.name.to_owned(),
            bounty: self.bounty,
        }
        .serialize(s)
    }
}

impl<'de> serde::Deserialize<'de> for crate::entities::Boss {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let w = BossWire::deserialize(d)?;
        let name = crate::field::BOSS_NAMES
            .iter()
            .copied()
            .find(|n| *n == w.name)
            .ok_or_else(|| D::Error::custom(format!("unknown boss {:?}", w.name)))?;
        Ok(crate::entities::Boss {
            name,
            bounty: w.bounty,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::entities::Boss;

    /// A known boss survives the round trip; an unknown name is refused.
    #[test]
    fn boss_names_round_trip_and_unknown_ones_are_refused() {
        let b = Boss {
            name: crate::field::BOSS_NAMES[1],
            bounty: 1200.0,
        };
        let bytes = to_json(&b);
        let back: Boss = serde_json::from_str(&bytes).expect("known boss");
        assert_eq!(back, b);
        let bad = bytes.replace(crate::field::BOSS_NAMES[1], "Nobody Special");
        assert!(serde_json::from_str::<Boss>(&bad).is_err());
    }

    /// JSON stands in for the wire format here (cj-net uses postcard); a
    /// plain name keeps the text substitution above simple.
    fn to_json(b: &Boss) -> String {
        serde_json::to_string(b).expect("encodes")
    }
}
