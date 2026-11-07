//! The salvage catalogue (ITEM_TYPES in src/js/01-tuning-constants.js).
//!
//! Each type has a radius, a base mass and weighted grades rolled at spawn.
//! The last grade of a multi-grade type is the rare one.

/// Kinds of salvage. Charges, ingots, keycards and cores belong to later
/// encounters but are listed so the catalogue matches the browser.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ItemType {
    Scrap,
    Ore,
    Plate,
    Engine,
    Reactor,
    Charge,
    Ingot,
    Blackbox,
    Pod,
    Keycard,
    Core,
}

/// A grade's sale value: fixed, or a range rolled skewed toward the low end.
#[derive(Clone, Copy, Debug)]
pub enum Value {
    Fixed(f64),
    Range(f64, f64),
}

#[derive(Clone, Copy, Debug)]
pub struct Grade {
    pub name: &'static str,
    pub value: Value,
    /// Relative spawn chance within the type.
    pub weight: f64,
    /// Mass multiplier for heavier premium pieces.
    pub mass_mul: f64,
    /// Hit-point override (a cracked reactor is more fragile).
    pub hp: Option<f64>,
    /// Outline colour as 0xRRGGBB; None uses the type's colour.
    pub tint: Option<u32>,
}

#[derive(Clone, Copy, Debug)]
pub struct ItemDef {
    pub r: f64,
    pub mass: f64,
    /// Volatile items prime and explode under hard impacts.
    pub volatile: bool,
    /// Hit points; None means indestructible (the browser's Infinity).
    pub hp: Option<f64>,
    /// Hidden value: shown as ??? until banked (cargo pods).
    pub hidden: bool,
    pub grades: &'static [Grade],
}

const fn g(name: &'static str, value: f64, weight: f64, tint: Option<u32>) -> Grade {
    Grade {
        name,
        value: Value::Fixed(value),
        weight,
        mass_mul: 1.0,
        hp: None,
        tint,
    }
}

const SCRAP: [Grade; 3] = [
    g("Bent scrap", 50.0, 5.0, None),
    g("Scrap", 80.0, 4.0, None),
    g("Alloy scrap", 160.0, 1.5, Some(0xd6e6ff)),
];
const ORE: [Grade; 4] = [
    g("Iron ore", 110.0, 5.0, Some(0xc9a77e)),
    g("Copper ore", 170.0, 3.0, Some(0xff9a5a)),
    g("Cobalt ore", 260.0, 1.5, Some(0x7f95ff)),
    g("Platinum ore", 450.0, 0.5, Some(0xeef4ff)),
];
const PLATE: [Grade; 3] = [
    g("Dented plate", 180.0, 3.0, Some(0x7d8aa0)),
    g("Hull plate", 280.0, 4.0, None),
    Grade {
        mass_mul: 1.3,
        ..g("Armor plate", 420.0, 1.5, Some(0xd6e6ff))
    },
];
const ENGINE: [Grade; 3] = [
    g("Burnt-out engine", 550.0, 2.0, Some(0x6b7a96)),
    g("Engine block", 850.0, 3.0, None),
    Grade {
        mass_mul: 1.15,
        ..g("Fusion drive", 1500.0, 1.0, Some(0x9fe4ff))
    },
];
const REACTOR: [Grade; 3] = [
    Grade {
        hp: Some(1.0),
        ..g("Cracked core", 900.0, 2.0, Some(0xe8d23b))
    },
    g("Reactor core", 1300.0, 3.0, None),
    g("Pristine core", 2200.0, 1.0, Some(0xe6ffd0)),
];
const CHARGE: [Grade; 1] = [g("Mining charge", 0.0, 1.0, None)];
const INGOT: [Grade; 3] = [
    g("Silver ingot", 500.0, 2.0, Some(0xdfe7f2)),
    g("Gold ingot", 750.0, 3.0, None),
    g("Iridium ingot", 1200.0, 1.0, Some(0xc9b6ff)),
];
const BLACKBOX: [Grade; 1] = [g("Flight recorder", 950.0, 1.0, None)];
const POD: [Grade; 1] = [Grade {
    value: Value::Range(200.0, 2000.0),
    ..g("Sealed cargo pod", 0.0, 1.0, None)
}];
const KEYCARD: [Grade; 1] = [g("Hold keycard", 0.0, 1.0, None)];
const CORE: [Grade; 1] = [g("Rogue core", 0.0, 1.0, None)];

const fn def(r: f64, mass: f64, grades: &'static [Grade]) -> ItemDef {
    ItemDef {
        r,
        mass,
        volatile: false,
        hp: None,
        hidden: false,
        grades,
    }
}

impl ItemType {
    pub fn def(self) -> &'static ItemDef {
        use ItemType::*;
        static SCRAP_D: ItemDef = def(12.0, 1.0, &SCRAP);
        static ORE_D: ItemDef = def(13.0, 1.5, &ORE);
        static PLATE_D: ItemDef = def(19.0, 3.0, &PLATE);
        static ENGINE_D: ItemDef = def(27.0, 7.0, &ENGINE);
        static REACTOR_D: ItemDef = ItemDef {
            volatile: true,
            hp: Some(3.0),
            ..def(21.0, 4.0, &REACTOR)
        };
        static CHARGE_D: ItemDef = ItemDef {
            volatile: true,
            hp: Some(2.0),
            ..def(14.0, 2.0, &CHARGE)
        };
        static INGOT_D: ItemDef = def(15.0, 3.0, &INGOT);
        static BLACKBOX_D: ItemDef = def(10.0, 0.5, &BLACKBOX);
        static POD_D: ItemDef = ItemDef {
            hidden: true,
            ..def(17.0, 2.5, &POD)
        };
        static KEYCARD_D: ItemDef = def(9.0, 0.5, &KEYCARD);
        static CORE_D: ItemDef = ItemDef {
            volatile: true,
            hp: Some(2.0),
            ..def(13.0, 2.0, &CORE)
        };
        match self {
            Scrap => &SCRAP_D,
            Ore => &ORE_D,
            Plate => &PLATE_D,
            Engine => &ENGINE_D,
            Reactor => &REACTOR_D,
            Charge => &CHARGE_D,
            Ingot => &INGOT_D,
            Blackbox => &BLACKBOX_D,
            Pod => &POD_D,
            Keycard => &KEYCARD_D,
            Core => &CORE_D,
        }
    }

    /// The browser's type key, used by the golden-trace fixtures.
    pub fn key(self) -> &'static str {
        use ItemType::*;
        match self {
            Scrap => "scrap",
            Ore => "ore",
            Plate => "plate",
            Engine => "engine",
            Reactor => "reactor",
            Charge => "charge",
            Ingot => "ingot",
            Blackbox => "blackbox",
            Pod => "pod",
            Keycard => "keycard",
            Core => "core",
        }
    }

    pub fn from_key(key: &str) -> Option<ItemType> {
        use ItemType::*;
        [
            Scrap, Ore, Plate, Engine, Reactor, Charge, Ingot, Blackbox, Pod, Keycard, Core,
        ]
        .into_iter()
        .find(|t| t.key() == key)
    }
}
