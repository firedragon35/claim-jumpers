//! The persistent career: wallet, loan, parts and levels, and the rules
//! that move money between shifts (src/js/14-game-state.js newCareer and
//! loadCareer; 18-hud-and-screens.js endShift, showPaidOff and the shop's
//! buy, mount and pay functions; 01-tuning-constants.js installments).
//!
//! The save file is JSON with the browser's field names. Loading repairs
//! anything malformed instead of trusting it, as loadCareer does. Fields
//! for systems that arrive in later milestones of the port (contracts,
//! crews, belts and the rest) are kept exactly as they were read, so a
//! save round-trips through this build without losing them.

use crate::js;
use crate::mods::{compute_mods, Mods};
use crate::parts::*;
use crate::world::World;
use serde_json::{json, Map, Value};

/// Difficulty modes (DIFFICULTY). The mode is chosen when a career starts
/// and is locked for that career.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Relaxed,
    Standard,
    Hard,
}

pub const MODES: [Mode; 3] = [Mode::Relaxed, Mode::Standard, Mode::Hard];

pub struct ModeDef {
    pub key: &'static str,
    pub label: &'static str,
    pub blurb: &'static str,
    /// × loan installment.
    pub installment: f64,
    /// × all hull damage taken.
    pub damage: f64,
    /// × rival arrival rate (the interval is divided by this).
    pub jumper_rate: f64,
    /// + most rivals alive at once.
    pub jumper_cap: f64,
    /// + mining crew size (milestone 4).
    pub miner_crew: i32,
    /// First shift debt collectors appear, and most waves per shift (milestone 4).
    pub collector_from: i32,
    pub collector_waves: i32,
    /// × how much each wiggle tears a net (milestone 4).
    pub net_tear: f64,
    /// × wreck deductible and impound fee.
    pub fees: f64,
}

impl Mode {
    pub fn def(self) -> &'static ModeDef {
        const RELAXED: ModeDef = ModeDef {
            key: "relaxed",
            label: "Relaxed",
            blurb: "Smaller installments, fewer and later threats, softer hits. Every encounter still shows up.",
            installment: 0.6,
            damage: 0.6,
            jumper_rate: 0.65,
            jumper_cap: -1.0,
            miner_crew: -1,
            collector_from: 3,
            collector_waves: 1,
            net_tear: 1.5,
            fees: 0.5,
        };
        const STANDARD: ModeDef = ModeDef {
            key: "standard",
            label: "Standard",
            blurb: "The intended curve: pressure builds shift by shift.",
            installment: 1.0,
            damage: 1.0,
            jumper_rate: 1.0,
            jumper_cap: 0.0,
            miner_crew: 0,
            collector_from: 2,
            collector_waves: 2,
            net_tear: 1.0,
            fees: 1.0,
        };
        const HARD: ModeDef = ModeDef {
            key: "hard",
            label: "Hard",
            blurb: "Bigger installments, more jumpers sooner, collectors from shift 1, and everything hits harder.",
            installment: 1.3,
            damage: 1.3,
            jumper_rate: 1.3,
            jumper_cap: 1.0,
            miner_crew: 1,
            collector_from: 1,
            collector_waves: 2,
            net_tear: 0.75,
            fees: 1.5,
        };
        match self {
            Mode::Relaxed => &RELAXED,
            Mode::Standard => &STANDARD,
            Mode::Hard => &HARD,
        }
    }

    pub fn from_key(key: &str) -> Option<Mode> {
        MODES.into_iter().find(|m| m.def().key == key)
    }
}

/// Faction standing (REP), each -100..100. The standing screen and most of
/// its events arrive with phase 6 in milestone 5; the bank's rate, the
/// rivals' grudge and the events that already exist are applied now.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rep {
    pub bank: f64,
    pub law: f64,
    pub rivals: f64,
}

pub const REP_LIMIT: f64 = 100.0;
pub const REP_GOOD: f64 = 40.0;
pub const REP_BAD: f64 = -40.0;
/// Reputation per event (REP).
pub const REP_SHIFT_SOLVENT_BANK: f64 = 4.0;
pub const REP_OVERDRAWN_BANK: f64 = -8.0;
pub const REP_EXTRA_PAYMENT_BANK: f64 = 2.0;
pub const REP_CLEAN_LAW: f64 = 3.0;
pub const REP_CLEAN_RIVALS: f64 = 2.0;
pub const REP_RIVAL_KILL: f64 = -3.0;
pub const REP_NAMES: [&str; 3] = ["Kessler Mutual", "Belt law", "Rival crews"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tier {
    Bad,
    Neutral,
    Good,
}

pub fn tier(v: f64) -> Tier {
    if v >= REP_GOOD {
        Tier::Good
    } else if v <= REP_BAD {
        Tier::Bad
    } else {
        Tier::Neutral
    }
}

/// Grudge kept at the end of each shift (rivals who respect you let go faster).
pub const GRUDGE_COOLING: f64 = 0.6;
pub const GRUDGE_COOLING_GOOD: f64 = 0.4;

/// The bank tolerates a debt of up to a third of what is still owed.
pub fn overdraft_limit(loan: f64) -> f64 {
    (loan.max(0.0) / 3.0).floor()
}

/// The installment charged at the end of shift `n` of a loan; it grows each
/// shift, scales with the mode, and rounds to a tidy 50 cr.
pub fn installment_for(n: i32, loan: f64, mode: Mode) -> f64 {
    loan.min(js::round((800.0 + 300.0 * f64::from(n - 1)) * mode.def().installment / 50.0) * 50.0)
}

/// Best single-shift rank titles (RANKS), by credits earned.
pub const RANKS: [(f64, &str); 5] = [
    (0.0, "Space litter"),
    (2500.0, "Weekend hauler"),
    (6000.0, "Licensed tugger"),
    (10000.0, "Claim boss"),
    (16000.0, "Legend of the belt"),
];

pub fn rank_for(earned: f64) -> &'static str {
    RANKS
        .iter()
        .rfind(|r| earned >= r.0)
        .map_or(RANKS[0].1, |r| r.1)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Career {
    pub shift: i32,
    pub wallet: f64,
    pub loan: f64,
    /// Parts owned, in the order bought (the cannon is always owned).
    pub owned: Vec<Part>,
    pub mounted: Mounted,
    pub total_banked: f64,
    pub mode: Mode,
    /// The rival crews' grudge, carried into the next shift.
    pub grudge: f64,
    pub levels: Levels,
    /// Sentry drones owned (milestone 4 launches them).
    pub sentries: u32,
    /// Index into TUG_CLASSES; goes up each time a loan is paid off.
    pub tug_class: usize,
    /// Shift the current loan started on (installments count from it).
    pub loan_start: i32,
    pub rep: Rep,
    /// Save fields for systems not yet ported, kept verbatim.
    pub extra: Map<String, Value>,
}

/// Fields this build reads and writes itself; everything else in a save
/// is carried through untouched.
const OWN_KEYS: [&str; 15] = [
    "shift",
    "wallet",
    "loan",
    "owned",
    "mounted",
    "totalBanked",
    "difficulty",
    "grudge",
    "wanted",
    "levels",
    "sentries",
    "tugClass",
    "loanStart",
    "rep",
    "lastShift",
];

/// JavaScript truthiness (`!!v`) for a JSON value.
fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Value::String(s) => !s.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

/// What the shift report shows and what happened to the career.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    /// The career continues to the machine shop.
    Continue,
    /// Past the overdraft limit: the tug is repossessed and the save cleared.
    Repossessed { over: f64 },
    /// The loan hit zero below the top class: traded up to the next class
    /// on a new loan (any overdraft rolled into it).
    TradedUp { from: usize, rolled: f64 },
    /// The top class paid off: the career is won and the save cleared.
    FleetPaid,
}

/// The end-of-shift settlement, for the report screen.
#[derive(Clone, Debug, PartialEq)]
pub struct Settlement {
    pub earned: f64,
    pub due: f64,
    /// Standing change this shift, as [bank, law, rivals].
    pub rep_delta: [f64; 3],
    pub outcome: Outcome,
}

/// Why a shop action was refused, or what it did.
pub type ShopResult = Result<String, String>;

impl Career {
    /// newCareer(mode): the starter tug with only the cutting cannon.
    pub fn new(mode: Mode) -> Career {
        let mut mounted: Mounted = [None; 5];
        mounted[0] = Some(Part::Cannon);
        Career {
            shift: 1,
            wallet: 0.0,
            loan: LOAN_START,
            owned: vec![Part::Cannon],
            mounted,
            total_banked: 0.0,
            mode,
            grudge: 0.0,
            levels: Levels::default(),
            sentries: 0,
            tug_class: 0,
            loan_start: 1,
            rep: Rep::default(),
            extra: Map::new(),
        }
    }

    pub fn tug(&self) -> &'static TugClass {
        &TUG_CLASSES[self.tug_class.min(TUG_CLASSES.len() - 1)]
    }

    pub fn mods(&self) -> Mods {
        compute_mods(&self.mounted, &self.levels, self.tug_class)
    }

    /// Kessler Mutual's rate adjustment on installments.
    pub fn bank_rate_mul(&self) -> f64 {
        match tier(self.rep.bank) {
            Tier::Good => 0.9,
            Tier::Bad => 1.15,
            Tier::Neutral => 1.0,
        }
    }

    /// The installment owed at the end of the current shift (or `ahead`
    /// shifts later). Trading up restarts the schedule at the first payment.
    pub fn due(&self, ahead: i32) -> f64 {
        let n = (self.shift - self.loan_start + 1 + ahead).max(1);
        let base = installment_for(n, self.loan, self.mode);
        self.loan
            .min(js::round(base * self.bank_rate_mul() / 50.0) * 50.0)
    }

    pub fn overdraft_limit(&self) -> f64 {
        overdraft_limit(self.loan)
    }

    pub fn owns(&self, p: Part) -> bool {
        self.owned.contains(&p)
    }

    pub fn is_mounted(&self, p: Part) -> bool {
        self.mounted.contains(&Some(p))
    }

    /// The first empty hardpoint of a type.
    pub fn free_slot(&self, kind: SlotType) -> Option<usize> {
        (0..SLOTS.len()).find(|&i| SLOTS[i].kind == kind && self.mounted[i].is_none())
    }

    /// A shop hint when a cannon mod is mounted without its cannon.
    fn cannon_note(&self, p: Part) -> &'static str {
        if p.def().slot == SlotType::Mod && !self.is_mounted(Part::Cannon) {
            " Mount the cutting cannon to use it."
        } else {
            ""
        }
    }

    pub fn buy_part(&mut self, p: Part) -> ShopResult {
        let d = p.def();
        if d.unique {
            return Err("Contract chain reward.".to_owned());
        }
        if let Some(when) = p.pending() {
            return Err(format!("{} arrives with {when} of the port.", d.name));
        }
        if self.owns(p) {
            return Err(format!("{} is already yours.", d.name));
        }
        if self.wallet < d.price {
            return Err(format!(
                "Need {} cr more.",
                crate::world::fmt_credits(d.price - self.wallet)
            ));
        }
        self.wallet -= d.price;
        self.owned.push(p);
        let slot = self.free_slot(d.slot);
        if let Some(i) = slot {
            self.mounted[i] = Some(p);
        }
        Ok(match slot {
            Some(_) => format!("{} bought and mounted.{}", d.name, self.cannon_note(p)),
            None => format!(
                "{} bought. Free a {} hardpoint to mount it.{}",
                d.name,
                d.slot.label(),
                self.cannon_note(p)
            ),
        })
    }

    pub fn mount(&mut self, p: Part) -> ShopResult {
        let d = p.def();
        if !self.owns(p) || self.is_mounted(p) || p.pending().is_some() {
            return Err(format!("{} can't be mounted.", d.name));
        }
        let Some(i) = self.free_slot(d.slot) else {
            return Err(format!(
                "No free {} hardpoint. Unmount something first.",
                d.slot.label()
            ));
        };
        self.mounted[i] = Some(p);
        Ok(format!("{} mounted.{}", d.name, self.cannon_note(p)))
    }

    pub fn unmount(&mut self, p: Part) -> ShopResult {
        if !self.is_mounted(p) {
            return Err(format!("{} isn't mounted.", p.def().name));
        }
        for m in &mut self.mounted {
            if *m == Some(p) {
                *m = None;
            }
        }
        Ok(if self.mods().guns() == 0 {
            format!(
                "{} unmounted. No guns mounted: you can still tow, ram and throw.",
                p.def().name
            )
        } else {
            format!("{} unmounted.", p.def().name)
        })
    }

    /// The price of the next level of a system, if it can be bought at all.
    pub fn level_price(&self, s: System) -> Option<f64> {
        if s.needs().is_some_and(|p| !self.owns(p)) {
            return None;
        }
        let lv = self.levels.get(s);
        (lv < s.max()).then(|| SYSTEM_PRICES[usize::from(lv - 1)])
    }

    pub fn buy_level(&mut self, s: System) -> ShopResult {
        let Some(price) = self.level_price(s) else {
            return Err(format!("{} can't be upgraded.", s.name()));
        };
        if self.wallet < price {
            return Err(format!(
                "Need {} cr more.",
                crate::world::fmt_credits(price - self.wallet)
            ));
        }
        self.wallet -= price;
        let lv = self.levels.get(s) + 1;
        self.levels.set(s, lv);
        Ok(format!("{} upgraded to level {lv}.", s.name()))
    }

    /// Buy one sentry drone (needs a mounted drone bay with room).
    pub fn buy_sentry(&mut self) -> ShopResult {
        if !self.is_mounted(Part::DroneBay) || self.sentries >= SENTRY_MAX {
            return Err("No room for a sentry.".to_owned());
        }
        if self.wallet < SENTRY_PRICE {
            return Err(format!(
                "Need {} cr more.",
                crate::world::fmt_credits(SENTRY_PRICE - self.wallet)
            ));
        }
        self.wallet -= SENTRY_PRICE;
        self.sentries += 1;
        Ok(format!(
            "Sentry drone added. {} of {SENTRY_MAX} in the bay.",
            self.sentries
        ))
    }

    /// An extra loan payment of up to `limit` cr. Paying early improves the
    /// bank's opinion of you (per whole 1,000 cr). Returns the trade-up if
    /// it cleared the loan.
    pub fn pay_extra(&mut self, limit: f64) -> Result<(String, Option<Outcome>), String> {
        let amount = limit.min(self.wallet).min(self.loan);
        if amount <= 0.0 {
            return Err("Nothing to pay with.".to_owned());
        }
        self.wallet -= amount;
        self.loan -= amount;
        let thousands = (amount / 1000.0).floor();
        if thousands > 0.0 {
            self.rep.bank =
                (self.rep.bank + REP_EXTRA_PAYMENT_BANK * thousands).clamp(-REP_LIMIT, REP_LIMIT);
        }
        if self.loan <= 0.0 {
            let o = self.paid_off();
            return Ok((String::new(), Some(o)));
        }
        Ok((
            format!(
                "Paid {} cr toward the loan.",
                crate::world::fmt_credits(amount)
            ),
            None,
        ))
    }

    /// The loan hit zero (showPaidOff): below the top class, trade up to the
    /// next class on a new loan with any overdraft rolled in; at the top,
    /// the career is won.
    pub fn paid_off(&mut self) -> Outcome {
        if self.tug_class < TUG_CLASSES.len() - 1 {
            let from = self.tug_class;
            self.tug_class += 1;
            let rolled = (-self.wallet).max(0.0);
            self.wallet = self.wallet.max(0.0);
            self.loan = self.tug().loan + rolled;
            // The new loan's first installment is due after the next shift.
            self.loan_start = self.shift;
            Outcome::TradedUp { from, rolled }
        } else {
            Outcome::FleetPaid
        }
    }

    fn set_last_shift_flag(&mut self, key: &str) {
        if let Some(Value::Object(ls)) = self.extra.get_mut("lastShift") {
            ls.insert(key.to_owned(), Value::Bool(true));
        }
    }

    /// Close out a shift (endShift): credit the haul, cool the grudge,
    /// charge the installment, update standing, and decide whether the
    /// career continues, is repossessed, or trades up.
    ///
    /// (Contract rewards, crew wages, chains and stored rogue cores join
    /// this settlement with their systems in milestone 5.)
    pub fn settle(&mut self, w: &World) -> Settlement {
        let earned = js::round(w.banked);
        // Collectors that confiscated a fat load credited it to the loan
        // during the shift (career.loan in the browser), before the
        // installment below is worked out.
        self.loan = (self.loan - w.loan_credit).max(0.0);
        let keep = if tier(self.rep.rivals) == Tier::Good {
            GRUDGE_COOLING_GOOD
        } else {
            GRUDGE_COOLING
        };
        self.grudge = (w.grudge * keep).floor();
        // Sentries that survived go back in the bay; destroyed or rogue
        // ones are gone for good.
        if w.mods.drone_bay {
            self.sentries = w.sentries.iter().filter(|d| !d.dead && !d.rogue).count() as u32;
        }
        // Unspent money stays in the wallet. The installment is always
        // taken; a negative wallet is an overdraft, allowed down to a third
        // of what is still owed (checked after the payment).
        let due = self.due(0);
        self.wallet += earned;
        self.total_banked += earned;
        self.extra.insert("contract".to_owned(), Value::Null);
        self.wallet -= due;
        self.loan -= due;

        // Standing: this shift's events plus end-of-shift standing.
        let mut d = w.rep_delta;
        if self.wallet >= 0.0 {
            d.bank += REP_SHIFT_SOLVENT_BANK;
        } else {
            d.bank += REP_OVERDRAWN_BANK;
        }
        if w.wanted == 0 && w.arrests == 0 && d.law == 0.0 {
            d.law += REP_CLEAN_LAW;
        }
        if w.stats.jumpers == 0 {
            d.rivals += REP_CLEAN_RIVALS;
        }
        let clamp = |v: f64| v.clamp(-REP_LIMIT, REP_LIMIT);
        self.rep = Rep {
            bank: clamp(self.rep.bank + d.bank),
            law: clamp(self.rep.law + d.law),
            rivals: clamp(self.rep.rivals + d.rivals),
        };

        // What Marge reacts to in the shop (milestone 5 reads it).
        self.extra.insert(
            "lastShift".to_owned(),
            json!({
                "earned": earned, "deaths": w.stats.deaths, "kills": w.stats.jumpers, "arrests": w.arrests,
                "wallet": self.wallet, "contract": null, "boss": null, "hold": false, "escort": false,
            }),
        );

        let over = -self.wallet - self.overdraft_limit();
        let outcome = if self.loan <= 0.0 {
            self.shift += 1;
            self.set_last_shift_flag("tradedUp");
            self.paid_off()
        } else if over > 0.0 {
            Outcome::Repossessed { over }
        } else {
            self.shift += 1;
            Outcome::Continue
        };
        Settlement {
            earned,
            due,
            rep_delta: [d.bank, d.law, d.rivals],
            outcome,
        }
    }

    /// Serialize for the save file, in the browser's field names, with the
    /// fields of systems not yet ported carried through.
    pub fn to_json(&self) -> String {
        let mut m = self.extra.clone();
        let mounted: Map<String, Value> = SLOTS
            .iter()
            .zip(self.mounted)
            .map(|(s, p)| {
                (
                    s.key.to_owned(),
                    p.map_or(Value::Null, |p| Value::from(p.def().key)),
                )
            })
            .collect();
        let levels: Map<String, Value> = SYSTEMS
            .iter()
            .map(|s| (s.key().to_owned(), Value::from(self.levels.get(*s))))
            .collect();
        let own = json!({
            "shift": self.shift,
            "wallet": self.wallet,
            "loan": self.loan,
            "owned": self.owned.iter().map(|p| p.def().key).collect::<Vec<_>>(),
            "mounted": mounted,
            "totalBanked": self.total_banked,
            "difficulty": self.mode.def().key,
            "grudge": self.grudge,
            "wanted": 0,
            "levels": levels,
            "sentries": self.sentries,
            "tugClass": self.tug_class,
            "loanStart": self.loan_start,
            "rep": { "bank": self.rep.bank, "law": self.rep.law, "rivals": self.rep.rivals },
        });
        if let Value::Object(o) = own {
            m.extend(o);
        }
        serde_json::to_string_pretty(&Value::Object(m)).expect("career serializes")
    }

    /// Load a saved career (loadCareer), repairing anything malformed.
    /// None if the text is not a JSON object at all.
    pub fn from_json(text: &str) -> Option<Career> {
        let raw: Value = serde_json::from_str(text).ok()?;
        let raw = raw.as_object()?;
        let mut c = Career::new(Mode::Standard);
        // num(v, d, min): finite and at least `min`, else the default.
        let num = |k: &str, d: f64, min: f64| {
            raw.get(k)
                .and_then(Value::as_f64)
                .filter(|v| v.is_finite() && *v >= min)
                .unwrap_or(d)
        };
        c.shift = num("shift", 1.0, 1.0).floor().min(f64::from(i32::MAX)) as i32;
        c.tug_class = (num("tugClass", 0.0, 0.0).floor() as usize).min(TUG_CLASSES.len() - 1);
        c.loan_start = (num("loanStart", 1.0, 1.0).floor().min(f64::from(c.shift)) as i32).max(1);
        // Standing: Number(v) || 0, clamped.
        let rep = raw.get("rep");
        let rep_of = |k: &str| {
            rep.and_then(|r| r.get(k))
                .and_then(Value::as_f64)
                .filter(|v| v.is_finite())
                .unwrap_or(0.0)
                .clamp(-REP_LIMIT, REP_LIMIT)
        };
        c.rep = Rep {
            bank: rep_of("bank"),
            law: rep_of("law"),
            rivals: rep_of("rivals"),
        };
        // A loan never exceeds its class's sticker price (old saves had a
        // bigger starter loan). The wallet may be overdrawn, but not past
        // the bank's limit.
        let cls_loan = c.tug().loan;
        c.loan = num("loan", cls_loan, 1.0);
        let class_zero = raw.get("tugClass").and_then(Value::as_f64).unwrap_or(0.0) == 0.0;
        if class_zero {
            c.loan = c.loan.min(cls_loan);
        }
        c.wallet = match raw
            .get("wallet")
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite())
        {
            Some(v) => v.max(-overdraft_limit(c.loan)),
            None => 0.0,
        };
        c.total_banked = num("totalBanked", 0.0, 0.0);
        // Saves from before modes existed load as Standard.
        c.mode = raw
            .get("difficulty")
            .and_then(Value::as_str)
            .and_then(Mode::from_key)
            .unwrap_or(Mode::Standard);
        c.grudge = num("grudge", 0.0, 0.0).floor();
        if let Some(Value::Object(lv)) = raw.get("levels") {
            for s in SYSTEMS {
                let v = lv
                    .get(s.key())
                    .and_then(Value::as_f64)
                    .filter(|v| v.is_finite() && *v >= 1.0)
                    .unwrap_or(1.0)
                    .floor()
                    .min(f64::from(s.max()));
                c.levels.set(s, v as u8);
            }
        }
        c.sentries = (num("sentries", 0.0, 0.0).floor() as u32).min(SENTRY_MAX);
        if let Some(Value::Array(owned)) = raw.get("owned") {
            for p in owned
                .iter()
                .filter_map(Value::as_str)
                .filter_map(Part::from_key)
            {
                if !c.owned.contains(&p) {
                    c.owned.push(p);
                }
            }
        }
        if let Some(Value::Object(src)) = raw.get("mounted") {
            let mut src: Map<String, Value> = src.clone();
            // Saves from when the twin cutter took a weapon hardpoint: move
            // it onto the cannon mod mount instead of dropping it.
            for k in ["w1", "w2"] {
                if src.get(k).and_then(Value::as_str) == Some("twin") {
                    src.insert(k.to_owned(), Value::Null);
                    if src.get("mod").is_none_or(Value::is_null) {
                        src.insert("mod".to_owned(), Value::from("twin"));
                    }
                }
            }
            let mut used = Vec::new();
            for (i, s) in SLOTS.iter().enumerate() {
                let p = src
                    .get(s.key)
                    .and_then(Value::as_str)
                    .and_then(Part::from_key);
                let ok = p.filter(|p| c.owns(*p) && p.def().slot == s.kind && !used.contains(p));
                c.mounted[i] = ok;
                if let Some(p) = ok {
                    used.push(p);
                }
            }
        }
        // What Marge reacts to: copied field by field, never trusted wholesale.
        if let Some(Value::Object(ls)) = raw.get("lastShift") {
            let n = |k: &str| {
                ls.get(k)
                    .and_then(Value::as_f64)
                    .filter(|v| v.is_finite() && *v >= 0.0)
                    .unwrap_or(0.0)
            };
            let contract = ls
                .get("contract")
                .and_then(Value::as_str)
                .filter(|s| *s == "done" || *s == "missed");
            let boss: Option<String> = ls
                .get("boss")
                .and_then(Value::as_str)
                .map(|s| s.chars().take(40).collect());
            let flag = |k: &str| ls.get(k).is_some_and(truthy);
            c.extra.insert(
                "lastShift".to_owned(),
                json!({
                    "earned": n("earned"), "deaths": n("deaths").floor(), "kills": n("kills").floor(),
                    "arrests": n("arrests").floor(),
                    "wallet": ls.get("wallet").and_then(Value::as_f64).filter(|v| v.is_finite()).unwrap_or(0.0),
                    "contract": contract, "boss": boss,
                    "hold": flag("hold"), "escort": flag("escort"), "tradedUp": flag("tradedUp"),
                }),
            );
        }
        // Everything else is for systems still to come: kept as it was.
        for (k, v) in raw {
            if !OWN_KEYS.contains(&k.as_str()) {
                c.extra.insert(k.clone(), v.clone());
            }
        }
        Some(c)
    }
}
