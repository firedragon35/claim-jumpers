//! Phase 6 and 7 systems of the browser game: contracts and contract
//! chains (src/js/08-contracts.js, 10b-standing.js), crews (09-crew.js),
//! the sector map's belts and their conditions (01-tuning-constants.js,
//! 10b-standing.js), truces, field events (10-field-events.js), escort
//! haulers, sealed holds and rogue cores (10c-salvage.js).

use crate::consts::RIVAL_STAGES;
use crate::consts::{RAD_CLOUD_RADIUS, RAD_HULL_DPS, RAD_SHIP_DPS};
use crate::items::ItemType;
use crate::js;
use crate::rng::Rng;

/* ------------------------------- Contracts ------------------------------ */

/// The kinds of job a contract can ask for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContractKind {
    Deliver,
    Bank,
    Rivals,
    Clean,
    Vault,
    Escort,
}

/// Every contract kind, in the browser's order.
pub const CONTRACT_KINDS: [ContractKind; 6] = [
    ContractKind::Deliver,
    ContractKind::Bank,
    ContractKind::Rivals,
    ContractKind::Clean,
    ContractKind::Vault,
    ContractKind::Escort,
];

impl ContractKind {
    /// The kind's key in the browser's save file.
    pub fn key(self) -> &'static str {
        match self {
            ContractKind::Deliver => "deliver",
            ContractKind::Bank => "bank",
            ContractKind::Rivals => "rivals",
            ContractKind::Clean => "clean",
            ContractKind::Vault => "vault",
            ContractKind::Escort => "escort",
        }
    }
    /// Looks a contract kind up by its save-file key; `None` if unknown.
    pub fn from_key(k: &str) -> Option<ContractKind> {
        CONTRACT_KINDS.into_iter().find(|c| c.key() == k)
    }
    /// Reward weight (CONTRACT_WEIGHT).
    fn weight(self) -> f64 {
        match self {
            ContractKind::Deliver => 1.0,
            ContractKind::Bank => 0.9,
            ContractKind::Rivals => 1.1,
            ContractKind::Clean => 1.3,
            ContractKind::Vault => 1.8,
            ContractKind::Escort => 1.4,
        }
    }
}

/// Cargo a delivery contract can ask for, with base counts (+1 every 4
/// shifts) and how the browser names it.
pub const DELIVER_COUNTS: [(ItemType, u32, &str); 4] = [
    (ItemType::Scrap, 6, "scrap"),
    (ItemType::Ore, 5, "ore"),
    (ItemType::Plate, 3, "hull plates"),
    (ItemType::Engine, 2, "engine blocks"),
];

/// One contract: the job, its target and its reward.
#[derive(Clone, Debug, PartialEq)]
pub struct Contract {
    pub kind: ContractKind,
    /// Target (count, credits, or 1 for yes/no jobs).
    pub n: f64,
    /// The cargo a delivery asks for.
    pub cargo: Option<ItemType>,
    pub reward: f64,
    /// A contract-chain step: the chain and step index.
    pub chain: Option<(Chain, usize)>,
}

/// makeContract(kind, shift): a fresh contract scaled to the shift.
pub fn make_contract(rng: &mut Rng, kind: ContractKind, shift: i32) -> Contract {
    let mut k = Contract {
        kind,
        n: 1.0,
        cargo: None,
        reward: 0.0,
        chain: None,
    };
    let s = f64::from(shift);
    match kind {
        ContractKind::Deliver => {
            let (t, base, _) = DELIVER_COUNTS[rng.index(DELIVER_COUNTS.len())];
            k.cargo = Some(t);
            k.n = f64::from(base) + (s / 4.0).floor();
        }
        ContractKind::Bank => k.n = 2000.0 + 500.0 * s,
        ContractKind::Rivals => k.n = 3.0 + (s / 3.0).floor(),
        ContractKind::Clean => k.n = 1500.0 + 400.0 * s,
        _ => {}
    }
    k.reward = js::round((500.0 + 150.0 * s) * kind.weight() / 50.0) * 50.0;
    k
}

impl Contract {
    /// What the job is, in plain words (contractJobText).
    pub fn job_text(&self) -> String {
        let f = crate::world::fmt_credits;
        match self.kind {
            ContractKind::Escort => "Escort a freight hauler to the bay".to_owned(),
            ContractKind::Deliver => {
                let name = DELIVER_COUNTS
                    .iter()
                    .find(|d| Some(d.0) == self.cargo)
                    .map_or("cargo", |d| d.2);
                format!("Deliver {} {name}", self.n)
            }
            ContractKind::Bank => format!("Bank {} cr in one shift", f(self.n)),
            ContractKind::Rivals => format!("Scrap {} rival jumpers", self.n),
            ContractKind::Clean => format!("Bank {} cr without wrecking the tug", f(self.n)),
            ContractKind::Vault => "Crack the Kessler vault".to_owned(),
        }
    }

    /// contractText: chain steps name their client and step.
    pub fn text(&self) -> String {
        match self.chain {
            Some((ch, step)) => format!(
                "{} {}/{}: {}",
                ch.def().client,
                step + 1,
                ch.def().steps.len(),
                self.job_text()
            ),
            None => self.job_text(),
        }
    }
}

/* -------------------------------- Chains -------------------------------- */

/// Contract chains: three linked jobs from one client; the last pays big
/// and unlocks a part you can't buy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chain {
    HollisFreight,
    SaltyDog,
}

/// Every contract chain.
pub const CHAINS: [Chain; 2] = [Chain::HollisFreight, Chain::SaltyDog];
/// Chains are offered from this shift, half the time a slot is free.
pub const CHAIN_FROM_SHIFT: i32 = 3;
pub const CHAIN_OFFER_CHANCE: f64 = 0.5;

/// The fixed data of a contract chain: client, steps, rewards and the part it unlocks.
pub struct ChainDef {
    pub key: &'static str,
    pub client: &'static str,
    pub part: crate::parts::Part,
    /// Each step's kind, cargo and target.
    pub steps: [(ContractKind, Option<ItemType>, f64); 3],
    pub rewards: [f64; 3],
}

impl Chain {
    /// The fixed data for this chain.
    pub fn def(self) -> &'static ChainDef {
        use crate::parts::Part;
        const HOLLIS: ChainDef = ChainDef {
            key: "hollisFreight",
            client: "Hollis Freight Co.",
            part: Part::HullSeal,
            steps: [
                (ContractKind::Deliver, Some(ItemType::Ore), 5.0),
                (ContractKind::Escort, None, 1.0),
                (ContractKind::Vault, None, 1.0),
            ],
            rewards: [900.0, 1400.0, 3200.0],
        };
        const SALTY: ChainDef = ChainDef {
            key: "saltyDog",
            client: "Salty Dog Salvage",
            part: Part::Grapple,
            steps: [
                (ContractKind::Deliver, Some(ItemType::Plate), 4.0),
                (ContractKind::Rivals, None, 5.0),
                (ContractKind::Clean, None, 3500.0),
            ],
            rewards: [800.0, 1300.0, 3000.0],
        };
        match self {
            Chain::HollisFreight => &HOLLIS,
            Chain::SaltyDog => &SALTY,
        }
    }
    /// Looks a chain up by its save-file key; `None` if unknown.
    pub fn from_key(k: &str) -> Option<Chain> {
        CHAINS.into_iter().find(|c| c.def().key == k)
    }
}

/// The contract for step `i` of a chain (chainContract).
pub fn chain_contract(ch: Chain, i: usize) -> Contract {
    let (kind, cargo, n) = ch.def().steps[i];
    Contract {
        kind,
        n,
        cargo,
        reward: ch.def().rewards[i],
        chain: Some((ch, i)),
    }
}

/* --------------------------------- Crew --------------------------------- */

/// One co-pilot at a time, paid a wage every shift, each with one perk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Crew {
    Rigger,
    Adjuster,
    Fixer,
    Broker,
}

/// Every crew member, in hiring-screen order.
pub const CREWS: [Crew; 4] = [Crew::Rigger, Crew::Adjuster, Crew::Fixer, Crew::Broker];
/// The broker's sale price, and the adjuster's fee multiplier.
pub const BROKER_RATE: f64 = 1.1;
pub const ADJUSTER_FEES: f64 = 0.4;

/// The fixed data of a crew member: who they are, their wage and their perk.
pub struct CrewDef {
    pub key: &'static str,
    pub name: &'static str,
    pub role: &'static str,
    pub wage: f64,
    pub perk: &'static str,
}

impl Crew {
    /// The fixed data for this crew member.
    pub fn def(self) -> &'static CrewDef {
        const RIGGER: CrewDef = CrewDef {
            key: "rigger",
            name: "Dex Ortega",
            role: "Rigger",
            wage: 300.0,
            perk: "Hooks from 30% farther and reels in 25% faster.",
        };
        const ADJUSTER: CrewDef = CrewDef {
            key: "adjuster",
            name: "Pilar Vance",
            role: "Claims adjuster",
            wage: 250.0,
            perk: "Deductibles and impound fees cut by 60%.",
        };
        const FIXER: CrewDef = CrewDef {
            key: "fixer",
            name: "Okafor",
            role: "Fixer",
            wage: 350.0,
            perk: "Rival crews hold half the grudge for each kill.",
        };
        const BROKER: CrewDef = CrewDef {
            key: "broker",
            name: "Yuki Tanaka",
            role: "Broker",
            wage: 400.0,
            perk: "Banked cargo sells for 10% more.",
        };
        match self {
            Crew::Rigger => &RIGGER,
            Crew::Adjuster => &ADJUSTER,
            Crew::Fixer => &FIXER,
            Crew::Broker => &BROKER,
        }
    }
    /// Looks a crew member up by key; `None` if unknown.
    pub fn from_key(k: &str) -> Option<Crew> {
        CREWS.into_iter().find(|c| c.def().key == k)
    }
}

/* ------------------------------ Belts ------------------------------ */

/// The five belts the sector map offers, from the safest to the most dangerous.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Belt {
    Quiet,
    Hollis,
    Kessler,
    Cinder,
    Frontier,
}

/// Every belt, in sector-map order.
pub const BELTS: [Belt; 5] = [
    Belt::Quiet,
    Belt::Hollis,
    Belt::Kessler,
    Belt::Cinder,
    Belt::Frontier,
];

/// What working a belt does (BELTS).
pub struct BeltDef {
    pub key: &'static str,
    pub name: &'static str,
    /// × every salvage item's sale value.
    pub value: f64,
    /// × rival arrival rate.
    pub rivals: f64,
    /// Extra (or fewer) debt-collector waves per shift.
    pub collectors: i32,
    /// No law at all past the beacon.
    pub lawless: bool,
    /// The rival grudge starts the shift at least this high.
    pub grudge_floor: f64,
    /// Forces this field event.
    pub event: Option<Event>,
    /// Tug class needed for the permit.
    pub unlock: usize,
    pub salvage: u8,
    pub danger: u8,
    /// Background wash (0xRRGGBB and alpha).
    pub tint: Option<(u32, f32)>,
    pub blurb: &'static str,
}

impl Belt {
    /// The fixed data for this belt.
    #[rustfmt::skip]
    pub fn def(self) -> &'static BeltDef {
        const QUIET: BeltDef = BeltDef { key: "quiet", name: "Quiet Drift", value: 0.75, rivals: 0.55, collectors: -1, lawless: false, grudge_floor: 0.0, event: None, unlock: 0,
            salvage: 1, danger: 1, tint: Some((0x28785a, 0.07)), blurb: "Picked over and peaceful. Few rivals, thin pickings." };
        const HOLLIS: BeltDef = BeltDef { key: "hollis", name: "Hollis Belt", value: 1.0, rivals: 1.0, collectors: 0, lawless: false, grudge_floor: 0.0, event: None, unlock: 0,
            salvage: 2, danger: 2, tint: None, blurb: "Home turf, next to the carrier. The usual mix." };
        const KESSLER: BeltDef = BeltDef { key: "kessler", name: "Kessler Deep", value: 1.2, rivals: 1.0, collectors: 1, lawless: false, grudge_floor: 0.0, event: None, unlock: 1,
            salvage: 3, danger: 3, tint: Some((0xffaa3c, 0.05)), blurb: "Rich ore, right under the bank’s nose. Collectors everywhere." };
        const CINDER: BeltDef = BeltDef { key: "cinder", name: "Cinder Reach", value: 1.35, rivals: 1.5, collectors: 0, lawless: false, grudge_floor: 2.0, event: None, unlock: 2,
            salvage: 4, danger: 4, tint: Some((0xff503c, 0.06)), blurb: "An old battlefield. Fat wrecks, and crews already annoyed." };
        const FRONTIER: BeltDef = BeltDef { key: "frontier", name: "The Frontier", value: 1.6, rivals: 1.7, collectors: 0, lawless: true, grudge_floor: 5.0, event: Some(Event::Boss), unlock: 3,
            salvage: 5, danger: 5, tint: Some((0xaa5aff, 0.07)), blurb: "No law past the beacon. Hostile crews, and a bounty boss every shift." };
        match self {
            Belt::Quiet => &QUIET,
            Belt::Hollis => &HOLLIS,
            Belt::Kessler => &KESSLER,
            Belt::Cinder => &CINDER,
            Belt::Frontier => &FRONTIER,
        }
    }
    /// Looks a belt up by its save-file key; `None` if unknown.
    pub fn from_key(k: &str) -> Option<Belt> {
        BELTS.into_iter().find(|b| b.def().key == k)
    }
}

/* ---------------------------- Belt conditions ---------------------------- */

/// A belt's condition for one shift; `roll_conditions` rolls one per belt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Condition {
    Normal,
    OreBoom,
    PatrolSweep,
    SalvageGlut,
    RivalSurge,
    IonFront,
    Night,
}

/// Every condition, in the browser's order.
pub const CONDITIONS: [Condition; 7] = [
    Condition::Normal,
    Condition::OreBoom,
    Condition::PatrolSweep,
    Condition::SalvageGlut,
    Condition::RivalSurge,
    Condition::IonFront,
    Condition::Night,
];

/// What a belt's condition does this shift (CONDITIONS).
pub struct ConditionDef {
    pub key: &'static str,
    pub name: &'static str,
    pub text: &'static str,
    pub weight: f64,
    /// × every sale value, and × ore and ingots (the ore boom).
    pub value: f64,
    pub ore_value: f64,
    /// × the rival arrival rate. Declared by the browser's rival surge but
    /// read nowhere in its code, so it has no effect there or here.
    pub rivals: f64,
    /// × the patrol delay (lower is more often), × shakedown odds.
    pub patrol: f64,
    pub shakedown: f64,
    /// Extra wrecks seeded at the start.
    pub wrecks: u32,
    /// × boss bounties.
    pub bounty: f64,
    pub event: Option<Event>,
    pub night: bool,
}

impl Condition {
    /// The fixed data for this condition.
    #[rustfmt::skip]
    pub fn def(self) -> &'static ConditionDef {
        const fn d(key: &'static str, name: &'static str, text: &'static str, weight: f64) -> ConditionDef {
            ConditionDef { key, name, text, weight, value: 1.0, ore_value: 1.0, rivals: 1.0, patrol: 1.0, shakedown: 1.0, wrecks: 0, bounty: 1.0, event: None, night: false }
        }
        const NORMAL: ConditionDef = d("none", "Normal", "", 40.0);
        const ORE_BOOM: ConditionDef = ConditionDef { ore_value: 1.5, ..d("oreBoom", "Ore boom", "Ore and ingots sell for 50% more.", 10.0) };
        const PATROL: ConditionDef = ConditionDef { patrol: 0.5, shakedown: 2.0, ..d("patrolSweep", "Patrol sweep", "Patrols twice as often. Shakedowns doubled.", 10.0) };
        const GLUT: ConditionDef = ConditionDef { value: 0.85, wrecks: 3, ..d("salvageGlut", "Salvage glut", "Three extra wrecks, but prices down 15%.", 10.0) };
        const SURGE: ConditionDef = ConditionDef { rivals: 1.4, bounty: 2.0, ..d("rivalSurge", "Rival surge", "Rivals 40% more often. Boss bounties doubled.", 10.0) };
        const ION: ConditionDef = ConditionDef { event: Some(Event::Ion), ..d("ionFront", "Ion front", "An ion storm is certain this shift.", 8.0) };
        const NIGHT: ConditionDef = ConditionDef { value: 1.3, night: true, ..d("night", "Night shift", "Low visibility and short-range radar. Everything pays 30% more.", 8.0) };
        match self {
            Condition::Normal => &NORMAL,
            Condition::OreBoom => &ORE_BOOM,
            Condition::PatrolSweep => &PATROL,
            Condition::SalvageGlut => &GLUT,
            Condition::RivalSurge => &SURGE,
            Condition::IonFront => &ION,
            Condition::Night => &NIGHT,
        }
    }
    /// Looks a condition up by its save-file key; `None` if unknown.
    pub fn from_key(k: &str) -> Option<Condition> {
        CONDITIONS.into_iter().find(|c| c.def().key == k)
    }
    /// The sale-value multiplier for one item type under this condition.
    pub fn value_for(self, kind: ItemType) -> f64 {
        let d = self.def();
        let by = if matches!(kind, ItemType::Ore | ItemType::Ingot) {
            d.ore_value
        } else {
            1.0
        };
        d.value * by
    }
}

/// Roll this shift's condition for every belt (none on shift 1); the
/// patrol sweep can't fall on a lawless belt.
pub fn roll_conditions(rng: &mut Rng, shift: i32) -> [Condition; 5] {
    let mut map = [Condition::Normal; 5];
    for (i, b) in BELTS.into_iter().enumerate() {
        if shift <= 1 {
            continue;
        }
        let pool: Vec<Condition> = CONDITIONS
            .into_iter()
            .filter(|c| !(*c == Condition::PatrolSweep && b.def().lawless))
            .collect();
        let total: f64 = pool.iter().map(|c| c.def().weight).sum();
        let mut roll = rng.next() * total;
        for c in pool {
            roll -= c.def().weight;
            if roll < 0.0 {
                map[i] = c;
                break;
            }
        }
    }
    map
}

/* ------------------------------ Field events ------------------------------ */

/// One field event per shift, partway through.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// Shields drop to zero and can't recharge for a while.
    Ion,
    /// A derelict freighter packed with high-value salvage drifts in.
    Derelict,
    /// A named rival boss crew comes hunting, with a bounty on it.
    Boss,
    /// A paid escort: a freight hauler crosses to the bay.
    Convoy,
}

/// Seconds into the shift when the event can fire, and the ion storm's length.
pub const EVENT_WINDOW: (f64, f64) = (80.0, 160.0);
/// Seconds an ion storm keeps shields from recharging.
pub const ION_STORM_TIME: f64 = 25.0;
/// Names a boss crew can be given; one is picked at random.
pub const BOSS_NAMES: [&str; 6] = [
    "\"Rust Queen\" Ivanna",
    "Big Sal Moreno",
    "Lucky Teodor",
    "The Pruitt Twins",
    "Mad Hattie Okonkwo",
    "Deacon Vrell",
];
/// Multiplier on a rival jumper's hit points when it becomes a boss.
pub const BOSS_HP_MUL: f64 = 4.0;

/// The derelict freighter's hold.
pub const FREIGHTER_CONTENTS: [ItemType; 11] = [
    ItemType::Engine,
    ItemType::Engine,
    ItemType::Reactor,
    ItemType::Plate,
    ItemType::Plate,
    ItemType::Plate,
    ItemType::Blackbox,
    ItemType::Pod,
    ItemType::Pod,
    ItemType::Ingot,
    ItemType::Ingot,
];

/* ------------------------------ Escort haulers ------------------------------ */

/// Hit points of an escorted freight hauler.
pub const HAULER_HP: f64 = 40.0;
/// Cruising speed of an escorted hauler, in px/s.
pub const HAULER_SPEED: f64 = 55.0;
/// Distance from the bay that counts as delivered.
pub const HAULER_ARRIVE: f64 = 220.0;
/// Paid when the escort wasn't a contract.
pub const ESCORT_BONUS: f64 = 900.0;
/// Seconds into the shift an escort contract's hauler appears.
pub const ESCORT_CONTRACT_AT: f64 = 45.0;

/* ------------------------------ Sealed holds ------------------------------ */

/// Chance, each shift from `LOCKED_FROM_SHIFT`, that one wreck is sealed.
pub const LOCKED_HOLD_CHANCE: f64 = 0.5;
/// First shift that can have a sealed wreck.
pub const LOCKED_FROM_SHIFT: i32 = 2;
/// What a sealed wreck holds once it is opened.
pub const LOCKED_CONTENTS: [ItemType; 8] = [
    ItemType::Ingot,
    ItemType::Ingot,
    ItemType::Engine,
    ItemType::Reactor,
    ItemType::Blackbox,
    ItemType::Pod,
    ItemType::Plate,
    ItemType::Plate,
];
/// The keycard spawns at least this far from its wreck.
pub const KEYCARD_MIN_DIST: f64 = 1400.0;

/* ------------------------------ Rogue cores ------------------------------ */

/// Credits paid for selling one stored rogue core.
pub const CORE_SELL: f64 = 900.0;
/// Per second while towed, a core zaps the tug.
pub const CORE_ZAP_CHANCE: f64 = 0.15;
/// Damage to the tug from one core zap.
pub const CORE_ZAP_DAMAGE: f64 = 4.0;
/// Most rogue cores a career can store.
pub const CORES_MAX: u32 = 9;

/* ------------------------------ Night shifts ------------------------------ */

/// How far the radar sees on a night shift, in px.
pub const NIGHT_RADAR: f64 = 1100.0;
/// Radius around the tug that stays fully lit on a night shift, in px.
pub const NIGHT_LIGHT: f64 = 200.0;
/// Radius at which the night darkness reaches full strength, in px.
pub const NIGHT_FADE: f64 = 560.0;

/* --------------------------------- Truces --------------------------------- */

/// The rivals' stage for a grudge (truceStage).
pub fn truce_stage(grudge: f64) -> usize {
    crate::consts::rival_stage_index(grudge)
}

/// The fixer's price to drop the grudge one stage; a hired fixer gets it
/// at half price.
pub fn truce_price(grudge: f64, fixer: bool) -> f64 {
    let base = 600.0 + 500.0 * truce_stage(grudge) as f64;
    js::round(base * if fixer { 0.5 } else { 1.0 } / 50.0) * 50.0
}

/// The grudge after a truce (or a boss kill): the bottom of the stage below.
pub fn grudge_one_stage_down(grudge: f64) -> f64 {
    let stage = truce_stage(grudge);
    if stage > 0 {
        RIVAL_STAGES[stage - 1].at
    } else {
        grudge
    }
}

/* ------------------------------ In the field ------------------------------ */

use crate::career::{tier, Career, Tier};
use crate::consts::{BAY_X, BAY_Y, CARRIER_X, CARRIER_Y, WORLD_H, WORLD_W};
use crate::entities::{make_wreck, Body, Boss};
use crate::events::{Radio, Sfx, Tint};
use crate::world::World;

/// An escorted freight hauler crossing the field to the bay.
#[derive(Clone, Debug)]
pub struct Hauler {
    pub b: Body,
    pub hp: f64,
    pub max_hp: f64,
    pub angle: f64,
    pub flash: f64,
    pub dead: bool,
    /// A convoy event pays a bonus on arrival; an escort contract pays
    /// through its reward instead.
    pub paid: bool,
}

impl Hauler {
    /// Does a bolt at (x, y) strike the hull? The shape is the one
    /// `foes::hauler` draws: a spine, wider containers and a tapered nose.
    pub fn hit_by_bolt(&self, x: f64, y: f64) -> bool {
        // The same 3 px of grace the bolt's own size gave the old circle.
        const PAD: f64 = 3.0;
        let (dx, dy) = (x - self.b.x, y - self.b.y);
        let (sin, cos) = self.angle.sin_cos();
        let (along, across) = (dx * cos + dy * sin, (dy * cos - dx * sin).abs());
        let half_width = if along > 24.0 {
            (40.0 - along) * 14.0 / 16.0
        } else if (-40.0..=10.0).contains(&along) {
            22.0
        } else {
            16.0
        };
        (-46.0 - PAD..=40.0 + PAD).contains(&along) && across < half_width + PAD
    }
}

impl World {
    /// The claims adjuster haggles deductibles and impound fees down.
    pub fn crew_fee_mul(&self) -> f64 {
        if self.crew == Some(Crew::Adjuster) {
            ADJUSTER_FEES
        } else {
            1.0
        }
    }

    /// Progress toward a contract this shift, in the contract's own units
    /// (items delivered, credits banked, rivals wrecked, or 0/1 flags).
    pub fn contract_progress(&self, k: &Contract) -> f64 {
        match k.kind {
            ContractKind::Deliver => self
                .delivered
                .iter()
                .find(|d| Some(d.0) == k.cargo)
                .map_or(0.0, |d| f64::from(d.1)),
            ContractKind::Bank => self.banked,
            ContractKind::Rivals => f64::from(self.stats.jumpers),
            // A clean shift counts what was banked, but only while the tug
            // has never been wrecked.
            ContractKind::Clean => {
                if self.stats.deaths > 0 {
                    0.0
                } else {
                    self.banked
                }
            }
            ContractKind::Escort => f64::from(u8::from(self.escort_done)),
            ContractKind::Vault => f64::from(u8::from(self.vault_opened)),
        }
    }

    /// The belt, its condition, the crew and the contract for a career's
    /// shift (the phase 6 part of newGame and applyCrew). Runs before the
    /// field is populated, so belt multipliers apply to the first rocks.
    pub(crate) fn setup_career_field(&mut self, c: &Career) {
        let bd = c.belt.def();
        self.belt = c.belt;
        self.belt_value = bd.value;
        self.belt_rivals = bd.rivals;
        // Conditions are rolled in the shop; a career that never opened
        // the sector map plays the belt under normal conditions.
        let i = BELTS.iter().position(|b| *b == c.belt).expect("listed");
        self.cond = match c.conditions {
            Some((s, map)) if s == c.shift => map[i],
            _ => Condition::Normal,
        };
        // Grudge carries over, at least the belt's floor; crews who hate
        // you start the shift already annoyed.
        let bad = tier(c.rep.rivals) == Tier::Bad;
        self.grudge = c.grudge.max(bd.grudge_floor) + if bad { 2.0 } else { 0.0 };
        self.crew = c.crew;
        if self.crew == Some(Crew::Rigger) {
            self.mods.hook_range *= 1.3;
            self.mods.reel *= 1.25;
        }
        self.contract = c.contract.clone().filter(|_| c.contract_shift == c.shift);
        if self
            .contract
            .as_ref()
            .is_some_and(|k| k.kind == ContractKind::Escort)
        {
            self.escort_at = Some(ESCORT_CONTRACT_AT);
        }
        self.event_t = self.rng.range(EVENT_WINDOW.0, EVENT_WINDOW.1);
        // The first shift is the quiet one: no field event.
        self.event_done = c.shift == 1;
    }

    /// After the field is seeded: a salvage glut's extra wrecks, and on
    /// some shifts a sealed hold with its keycard out in the field.
    pub(crate) fn setup_after_populate(&mut self) {
        let extra = self.cond.def().wrecks;
        let (mut n, mut tries) = (0, 0);
        while n < extra && tries < 200 {
            tries += 1;
            let x = self.rng.range(300.0, WORLD_W - 300.0);
            let y = self.rng.range(300.0, WORLD_H - 300.0);
            if self.clear_spot(x, y, 160.0, 1100.0) {
                let w = make_wreck(&mut self.rng, x, y, self.shift_no);
                self.wrecks.push(w);
                n += 1;
            }
        }
        if self.shift_no >= LOCKED_FROM_SHIFT && self.rng.next() < LOCKED_HOLD_CHANCE {
            self.setup_locked_hold();
        }
    }

    /// Radiation clouds: inside one the tug's shields are out and its hull
    /// burns; other ships burn too, and none of those kills are the player's.
    fn update_radiation(&mut self, dt: f64) {
        for c in &mut self.clouds {
            c.t -= dt;
        }
        self.clouds.retain(|c| c.t > 0.0);
        if self.clouds.is_empty() {
            self.in_cloud = false;
            return;
        }
        let clouds = self.clouds.clone();
        let inside = |b: &crate::entities::Body| {
            clouds
                .iter()
                .any(|c| js::dist(c.x, c.y, b.x, b.y) < RAD_CLOUD_RADIUS)
        };
        let n = RAD_SHIP_DPS * dt;
        self.without_blame(|w| {
            for i in 0..w.jumpers.len() {
                if inside(&w.jumpers[i].b) {
                    w.hurt_jumper(i, n, None);
                }
            }
            for i in 0..w.collectors.len() {
                if inside(&w.collectors[i].b) {
                    w.hurt_collector(i, n, None);
                }
            }
            for i in 0..w.sheriffs.len() {
                if inside(&w.sheriffs[i].b) {
                    w.hurt_sheriff(i, n, None);
                }
            }
            for i in 0..w.miners.len() {
                if inside(&w.miners[i].b) {
                    w.hurt_miner(i, n, false);
                }
            }
        });
        let now_inside = self.ship.alive && inside(&self.ship.b);
        if now_inside && !self.in_cloud {
            self.radio_text(
                Radio::Radiation,
                true,
                false,
                "That's radiation, hotshot. Your shields are useless in there and the hull is cooking. Get out.".to_owned(),
            );
        }
        self.in_cloud = now_inside;
        if now_inside && self.ship.invuln <= 0.0 {
            let delay = self.mods.shield_delay;
            let burn = RAD_HULL_DPS * self.diff_damage * dt;
            let s = &mut self.ship;
            s.shield = 0.0;
            s.shield_delay_t = delay;
            s.hp -= burn;
            if s.hp <= 0.0 {
                self.kill_ship();
            }
        }
    }

    /// Per-step field systems (updateEvents): the self-sealing hull, the
    /// escort, cores on the line, the ion storm, and the shift's event.
    pub(crate) fn update_events(&mut self, dt: f64) {
        self.update_radiation(dt);
        let max_hp = self.mods.max_hp;
        let s = &mut self.ship;
        if s.alive && self.mods.hull_seal && s.hp < max_hp {
            s.hp = max_hp.min(s.hp + 1.5 * dt);
        }
        if self.hauler.is_none() && self.escort_at.is_some_and(|t| self.t >= t) {
            self.spawn_hauler(false);
        }
        self.update_hauler(dt);
        self.update_cores(dt);
        if self.ion_t > 0.0 {
            self.ion_t -= dt;
            self.ship.shield = 0.0;
            if self.ion_t <= 0.0 {
                self.radio(Radio::IonClear, true, false);
            }
        }
        if self.tut.is_some() || self.event_done || !self.ship.alive {
            return;
        }
        self.event_t -= dt;
        if self.event_t > 0.0 {
            return;
        }
        self.event_done = true;
        // A belt condition or the belt itself can force the event; a convoy
        // is only offered when no escort is already running.
        let forced = self
            .forced_event
            .or(self.cond.def().event)
            .or(self.belt.def().event);
        let kind = match forced {
            Some(k) => k,
            None => {
                let pool: &[Event] = if self.hauler.is_some() {
                    &[Event::Ion, Event::Derelict, Event::Boss]
                } else {
                    &[Event::Ion, Event::Derelict, Event::Boss, Event::Convoy]
                };
                pool[self.rng.index(pool.len())]
            }
        };
        self.start_event(kind);
    }

    /// Start a field event now (also used by tests to force one).
    pub fn start_event(&mut self, kind: Event) {
        match kind {
            Event::Convoy => self.spawn_hauler(true),
            Event::Ion => {
                self.ion_t = ION_STORM_TIME;
                if self.ship.shield > 0.0 {
                    self.sfx(Sfx::ShieldDown);
                }
                self.ship.shield = 0.0;
                self.radio(Radio::IonStorm, true, false);
            }
            Event::Derelict => {
                let (px, py) = self.edge_point(1300.0);
                let mut w = make_wreck(
                    &mut self.rng,
                    px.clamp(250.0, WORLD_W - 250.0),
                    py.clamp(250.0, WORLD_H - 250.0),
                    self.shift_no,
                );
                w.freighter = true;
                w.contents = FREIGHTER_CONTENTS.to_vec();
                w.hp = 60.0;
                w.max_hp = 60.0;
                // The freighter drifts toward the carrier, so ignoring it
                // still brings it into reach eventually.
                let a = (CARRIER_Y - w.b.y).atan2(CARRIER_X - w.b.x);
                w.b.vx = a.cos() * 20.0;
                w.b.vy = a.sin() * 20.0;
                self.wrecks.push(w);
                self.radio(Radio::Derelict, true, false);
            }
            Event::Boss => {
                let (px, py) = self.edge_point(1300.0);
                let variant = if self.rng.next() < 0.5 {
                    crate::consts::Variant::Gunship
                } else {
                    crate::consts::Variant::Brute
                };
                let x = px.clamp(40.0, WORLD_W - 40.0);
                let y = py.clamp(40.0, WORLD_H - 40.0);
                let mut j = self.new_jumper(x, y, Some(variant));
                j.hp *= BOSS_HP_MUL;
                j.hunter = true;
                let name = BOSS_NAMES[self.rng.index(BOSS_NAMES.len())];
                let base = 1200.0 + 200.0 * f64::from(self.shift_no);
                let bounty = js::round(base * self.cond.def().bounty);
                j.boss = Some(Boss { name, bounty });
                self.jumpers.push(j);
                let v = crate::world::fmt_credits(bounty);
                let text = format!(
                    "That's {name}'s crew. Bounty of {v} cr on them, and they're coming for you, not the cargo."
                );
                self.radio_text(Radio::Boss, true, false, text);
            }
        }
    }

    fn spawn_hauler(&mut self, paid: bool) {
        let (px, py) = self.edge_point(1600.0);
        self.hauler = Some(Hauler {
            b: Body {
                x: px.clamp(60.0, WORLD_W - 60.0),
                y: py.clamp(60.0, WORLD_H - 60.0),
                vx: 0.0,
                vy: 0.0,
                r: 34.0,
                mass: 1.0,
                fixed: false,
            },
            hp: HAULER_HP,
            max_hp: HAULER_HP,
            angle: 0.0,
            flash: 0.0,
            dead: false,
            paid,
        });
        self.radio(Radio::Escort, true, false);
    }

    /// The hauler flies itself to the bay; arriving completes the escort.
    fn update_hauler(&mut self, dt: f64) {
        let Some(h) = self.hauler.as_mut().filter(|h| !h.dead) else {
            return;
        };
        h.flash = (h.flash - dt).max(0.0);
        crate::rivals::steer(&mut h.b, &mut h.angle, BAY_X, BAY_Y, HAULER_SPEED, 40.0, dt);
        h.b.x += h.b.vx * dt;
        h.b.y += h.b.vy * dt;
        if js::dist(h.b.x, h.b.y, BAY_X, BAY_Y) >= HAULER_ARRIVE {
            return;
        }
        h.dead = true;
        let paid = h.paid;
        self.escort_done = true;
        self.rep_delta.bank += crate::career::REP_ESCORT_DONE_BANK;
        if paid {
            self.banked += ESCORT_BONUS;
            let text = format!("ESCORT +{} CR", crate::world::fmt_credits(ESCORT_BONUS));
            self.popup(BAY_X, BAY_Y - 60.0, text, Tint::Hazard);
        }
        self.sfx(Sfx::Jackpot);
        self.radio(Radio::EscortDone, true, false);
    }

    /// A rival bolt (or anything else) hit the hauler.
    pub fn hurt_hauler(&mut self, n: f64) {
        let Some(h) = self.hauler.as_mut().filter(|h| !h.dead) else {
            return;
        };
        h.hp -= n;
        h.flash = 0.12;
        if h.hp > 0.0 {
            if h.hp < h.max_hp * 0.4 {
                self.radio(Radio::EscortLow, false, true);
            }
            return;
        }
        h.dead = true;
        let b = h.b;
        self.escort_failed = true;
        self.explosion_fx(b.x, b.y, 160.0, Tint::Danger);
        self.sfx(Sfx::Boom);
        // What survives of the cargo is scrap for whoever gets there first.
        for _ in 0..3 {
            let x = b.x + self.rng.range(-20.0, 20.0);
            let y = b.y + self.rng.range(-20.0, 20.0);
            let (vx, vy) = (self.rng.range(-60.0, 60.0), self.rng.range(-60.0, 60.0));
            let it = self.new_item(ItemType::Scrap, x, y, vx, vy);
            self.items.push(it);
        }
        self.radio(Radio::EscortLost, true, false);
    }

    /// Seal one wreck and hide its keycard far away in the field.
    pub fn setup_locked_hold(&mut self) {
        let candidates: Vec<usize> = (0..self.wrecks.len())
            .filter(|&i| !self.wrecks[i].freighter)
            .collect();
        if candidates.is_empty() {
            return;
        }
        let w = candidates[self.rng.index(candidates.len())];
        self.wrecks[w].locked = true;
        self.wrecks[w].contents = LOCKED_CONTENTS.to_vec();
        let wb = self.wrecks[w].b;
        for _ in 0..60 {
            let x = self.rng.range(250.0, WORLD_W - 250.0);
            let y = self.rng.range(250.0, WORLD_H - 250.0);
            let near_hold = js::dist(x, y, wb.x, wb.y) < KEYCARD_MIN_DIST;
            if near_hold || js::dist(x, y, CARRIER_X, CARRIER_Y) < 800.0 {
                continue;
            }
            let (vx, vy) = (self.rng.range(-6.0, 6.0), self.rng.range(-6.0, 6.0));
            let it = self.new_item(ItemType::Keycard, x, y, vx, vy);
            self.items.push(it);
            return;
        }
        // Nowhere sensible to put the card: leave the wreck ordinary.
        self.wrecks[w].locked = false;
    }

    /// The keycard touched its wreck: open the hold and spill everything.
    pub(crate) fn unlock_hold(&mut self, w: usize, card: usize) {
        self.wrecks[w].locked = false;
        self.remove_item(card);
        let b = self.wrecks[w].b;
        self.popup(
            b.x,
            b.y - b.r * 0.8 - 30.0,
            "HOLD OPEN".to_owned(),
            Tint::Cable,
        );
        self.sfx(Sfx::Jackpot);
        self.break_wreck(w);
        self.hold_opened = true;
        self.emit(crate::events::Event::Highlight {
            score: 1600.0,
            text: "Opened a sealed hold".to_owned(),
        });
        self.radio(Radio::HoldOpen, true, false);
    }

    /// Shooting or ramming a sealed hold does nothing; Marge says why, once.
    pub(crate) fn sealed_hold_hit(&mut self, w: usize) {
        let b = self.wrecks[w].b;
        self.burst(b.x, b.y, 4, Tint::Text, 120.0);
        self.radio(Radio::HoldSealed, false, true);
    }

    /// A rogue core on the line rolls once a second to zap the tug.
    fn update_cores(&mut self, dt: f64) {
        let ids = self.chain.clone();
        for id in ids {
            let Some(i) = self.item_index(id) else {
                continue;
            };
            if self.items[i].kind != ItemType::Core {
                continue;
            }
            let it = &mut self.items[i];
            it.zap_t -= dt;
            if it.zap_t > 0.0 {
                continue;
            }
            it.zap_t = 1.0;
            let (x, y) = (it.b.x, it.b.y);
            if self.rng.next() >= CORE_ZAP_CHANCE {
                continue;
            }
            self.damage_ship(CORE_ZAP_DAMAGE);
            self.burst(x, y, 6, Tint::Danger, 120.0);
            self.sfx(Sfx::BeamZap);
            self.radio(Radio::CoreZap, false, true);
        }
    }

    /// A core reached the bay: stored for the shop, not sold here.
    pub(crate) fn store_core(&mut self, i: usize) {
        self.cores_banked += 1;
        self.stats.items += 1;
        let (x, y) = (self.items[i].b.x, self.items[i].b.y);
        self.popup(x, y - 20.0, "CORE STORED".to_owned(), Tint::Danger);
        self.burst(x, y, 20, Tint::Danger, 180.0);
        self.remove_item(i);
        self.sfx(Sfx::Bank);
    }
}
