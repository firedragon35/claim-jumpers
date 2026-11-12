//! How simulation types travel on the network (cj-net), where that differs
//! from a plain derive: boss crew names go as text and are read back from
//! `field::BOSS_NAMES` (so a message can't introduce an unknown name), and
//! the numbers that fill a snapshot go at less than their eight bytes: where
//! things are and how they move as f32 (see [`f32_wire`]), and timers and
//! levels that mostly rest at zero as one byte while they do (see
//! [`f32_sparse`]).
//!
//! A server sends every player a snapshot 20 times a second, and a host's
//! upload has to carry all of them, so these bytes are what limits how many
//! players a home connection can host. Only the tugs, salvage, rocks and
//! shots are packed this way: they are nearly all of a snapshot. Hit points,
//! shields and credits stay f64, so what a client shows of them is exactly
//! what the server holds.

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

/// `#[serde(with = ...)]` for an f64 that is usually exactly zero (a timer
/// that isn't running, a cable that isn't pulling): zero travels as nothing
/// but its one-byte marker, anything else as that marker and an f32. A
/// negative zero arrives as plain zero; nothing in the simulation reads the
/// sign of a zero.
pub mod f32_sparse {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(v: &f64, s: S) -> Result<S::Ok, S::Error> {
        // An Option is the one-byte marker: None for zero.
        (*v != 0.0).then_some(*v as f32).serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
        Option::<f32>::deserialize(d).map(|v| v.map_or(0.0, f64::from))
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

    /// A number sent packed comes back as itself to f32 precision, and a
    /// zero sent sparse is only its marker (JSON's `null` here).
    #[test]
    fn packed_numbers_round_trip() {
        #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
        struct Packed {
            #[serde(with = "super::f32_wire")]
            dense: f64,
            #[serde(with = "super::f32_sparse")]
            resting: f64,
            #[serde(with = "super::f32_sparse")]
            running: f64,
        }
        let sent = Packed {
            dense: 5199.123456789,
            resting: 0.0,
            running: -0.75,
        };
        let text = serde_json::to_string(&sent).expect("encodes");
        assert!(text.contains("\"resting\":null"), "{text}");
        let back: Packed = serde_json::from_str(&text).expect("decodes");
        assert!((back.dense - sent.dense).abs() < 0.001, "{}", back.dense);
        assert_eq!((back.resting, back.running), (0.0, -0.75));
        // A negative zero is still zero, and so still only a marker.
        let zero = Packed {
            dense: 0.0,
            resting: -0.0,
            running: 0.0,
        };
        let text = serde_json::to_string(&zero).expect("encodes");
        assert!(text.contains("\"resting\":null"), "{text}");
    }

    /// JSON stands in for the wire format here (cj-net uses postcard); a
    /// plain name keeps the text substitution above simple.
    fn to_json(b: &Boss) -> String {
        serde_json::to_string(b).expect("encodes")
    }
}
