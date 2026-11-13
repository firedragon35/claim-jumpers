//! The pilot's profile across every career (src/js/06-profile.js):
//! lifetime stats, the kill tally and achievements, which survive
//! repossession; plus the pilot name and the local leaderboard
//! (src/js/18-hud-and-screens.js). Stored as the browser stores them.

use cj_sim::world::{Kills, Stats};
use serde_json::{json, Map, Value};

/// [id, title, how to earn it].
pub const ACHIEVEMENTS: [(&str, &str, &str); 13] = [
    ("trained", "Licensed to tow", "Finish Marge's training."),
    (
        "firstBank",
        "First paycheck",
        "Bank your first piece of salvage.",
    ),
    (
        "smash",
        "Blunt instrument",
        "Wreck a rival jumper with swung cargo.",
    ),
    (
        "bigHaul",
        "Heavy lifting",
        "Bank 5,000 cr in a single delivery.",
    ),
    ("vault", "Safecracker", "Crack the Kessler vault."),
    ("contract", "Contractor", "Complete a contract."),
    ("boss", "Bounty hunter", "Take down a rival boss crew."),
    ("rogue", "Clean-up crew", "Destroy a rogue sentry."),
    ("fiveStar", "Most wanted", "Reach five wanted stars."),
    (
        "clean",
        "Not a scratch",
        "Bank 3,000 cr in a shift without wrecking the tug.",
    ),
    (
        "frontier",
        "Past the beacon",
        "Finish a shift in the Frontier.",
    ),
    ("tradeUp", "Upwardly mobile", "Pay off a tug and trade up."),
    ("fleet", "Owner-operator", "Pay off the Mk V."),
];

pub const PROFILE_STATS: [(&str, &str); 7] = [
    ("shifts", "Shifts flown"),
    ("banked", "Total banked (cr)"),
    ("best", "Best single shift (cr)"),
    ("kills", "Rivals scrapped"),
    ("bosses", "Boss crews taken down"),
    ("contracts", "Contracts completed"),
    ("deaths", "Tugs wrecked"),
];

/// [key, plural label]: the lifetime kill tally (KILL_KINDS).
pub const KILL_KINDS: [(&str, &str); 11] = [
    ("skiff", "Skiffs"),
    ("cutter", "Cutters"),
    ("brute", "Brutes"),
    ("gunship", "Gunships"),
    ("boss", "Boss crews"),
    ("collector", "Debt collectors"),
    ("law", "Law units"),
    ("miner", "Mining drones"),
    ("drone", "Refinery drones"),
    ("rogue", "Rogue sentries"),
    ("jaws", "Kessler jaws"),
];

/// A shift's kills in KILL_KINDS order.
pub fn kill_counts(k: &Kills) -> [u32; 11] {
    [
        k.skiff,
        k.cutter,
        k.brute,
        k.gunship,
        k.boss,
        k.collector,
        k.law,
        k.miner,
        k.drone,
        k.rogue,
        k.jaws,
    ]
}

/// Paint jobs (PAINTS): [id, name, hull, trim, achievement that unlocks it].
pub const PAINTS: [(&str, &str, u32, u32, Option<&str>); 8] = [
    ("class", "Class colours", 0, 0, None),
    (
        "hazard",
        "Hazard stripes",
        0x2a2206,
        0xffd23d,
        Some("firstBank"),
    ),
    ("midnight", "Midnight", 0x0b0f1e, 0x7d8cff, Some("trained")),
    ("rust", "Rust bucket", 0x2b1408, 0xd9733b, Some("smash")),
    (
        "gold",
        "Safecracker gold",
        0x2c2408,
        0xffe066,
        Some("vault"),
    ),
    (
        "wanted",
        "Most wanted",
        0x101828,
        0xff5a4d,
        Some("fiveStar"),
    ),
    ("bounty", "Bounty hunter", 0x1c0d2a, 0xc78bff, Some("boss")),
    (
        "chrome",
        "Owner-operator chrome",
        0x3a4250,
        0xe8eef8,
        Some("fleet"),
    ),
];

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Profile {
    pub stats: [f64; 7],
    /// Achievement id → when it was earned (ms since the epoch).
    pub got: Vec<(&'static str, f64)>,
    pub paint: String,
    pub kills: [u32; 11],
}

impl Profile {
    /// Profile.load: non-negative finite stats, known achievements, a
    /// plain lower-case paint id.
    pub fn from_json(text: Option<&str>) -> Profile {
        let raw = text
            .and_then(|t| serde_json::from_str::<Value>(t).ok())
            .unwrap_or(Value::Null);
        let mut p = Profile {
            paint: "class".to_owned(),
            ..Profile::default()
        };
        for (i, (k, _)) in PROFILE_STATS.iter().enumerate() {
            if let Some(v) = raw["stats"][k]
                .as_f64()
                .filter(|v| v.is_finite() && *v >= 0.0)
            {
                p.stats[i] = v;
            }
        }
        for (id, _, _) in ACHIEVEMENTS {
            if let Some(t) = raw["got"][id].as_f64().filter(|t| t.is_finite()) {
                p.got.push((id, t));
            }
        }
        if let Some(s) = raw["paint"]
            .as_str()
            .filter(|s| (1..=16).contains(&s.len()) && s.chars().all(|c| c.is_ascii_lowercase()))
        {
            p.paint = s.to_owned();
        }
        for (i, (k, _)) in KILL_KINDS.iter().enumerate() {
            if let Some(v) = raw["kills"][k]
                .as_f64()
                .filter(|v| v.is_finite() && *v >= 0.0)
            {
                p.kills[i] = v.floor() as u32;
            }
        }
        p
    }

    pub fn to_json(&self) -> String {
        let stats: Map<String, Value> = PROFILE_STATS
            .iter()
            .zip(self.stats)
            .map(|((k, _), v)| ((*k).to_owned(), Value::from(v)))
            .collect();
        let got: Map<String, Value> = self
            .got
            .iter()
            .map(|(k, t)| ((*k).to_owned(), Value::from(*t)))
            .collect();
        let kills: Map<String, Value> = KILL_KINDS
            .iter()
            .zip(self.kills)
            .map(|((k, _), v)| ((*k).to_owned(), Value::from(v)))
            .collect();
        json!({ "stats": stats, "got": got, "paint": self.paint, "kills": kills }).to_string()
    }

    pub fn has(&self, id: &str) -> bool {
        self.got.iter().any(|g| g.0 == id)
    }

    /// Unlock an achievement once; returns its title when it is new.
    pub fn unlock(&mut self, id: &str, now_ms: f64) -> Option<&'static str> {
        let (key, title, _) = ACHIEVEMENTS.iter().find(|a| a.0 == id)?;
        if self.has(id) {
            return None;
        }
        self.got.push((key, now_ms));
        Some(title)
    }

    pub fn bump(&mut self, stat: &str, n: f64) {
        if let Some(i) = PROFILE_STATS.iter().position(|s| s.0 == stat) {
            self.stats[i] += n;
        }
    }

    /// Adds one flight's fighting tallies (deaths, rivals scrapped, bosses
    /// and kills by hull) to the lifetime records. Both career shifts and
    /// daily challenges count here; career-only stats (shifts, banked, best
    /// shift) and achievements stay with the career path.
    pub fn add_fighting(&mut self, stats: &Stats, kills: &Kills) {
        self.bump("deaths", f64::from(stats.deaths));
        self.bump("kills", f64::from(stats.jumpers));
        self.bump("bosses", f64::from(kills.boss));
        for (i, n) in kill_counts(kills).into_iter().enumerate() {
            self.kills[i] += n;
        }
    }

    pub fn best(&mut self, n: f64) {
        self.stats[2] = self.stats[2].max(n);
    }

    /// The chosen paint job's (hull, trim), if it is unlocked and not the
    /// class colours.
    pub fn paint_colours(&self) -> Option<(u32, u32)> {
        let p = PAINTS.iter().find(|p| p.0 == self.paint)?;
        let open = p.4.is_none_or(|a| self.has(a));
        (open && p.0 != "class").then_some((p.2, p.3))
    }
}

pub const BOARD_SIZE: usize = 20;

// The name rule is shared with the server, which cleans names sent to it.
pub use cj_net::{clean_name, NAME_MAX};

/// One local leaderboard row.
#[derive(Clone, Debug, PartialEq)]
pub struct Score {
    pub name: String,
    /// A difficulty key, or "daily-YYYY-MM-DD".
    pub mode: String,
    pub score: f64,
    pub shift: i64,
    pub at: f64,
}

/// loadLocalBoard: the stored rows, skipping anything malformed.
pub fn board_from_json(text: Option<&str>) -> Vec<Score> {
    let Some(Value::Array(rows)) = text.and_then(|t| serde_json::from_str::<Value>(t).ok()) else {
        return Vec::new();
    };
    rows.iter()
        .filter_map(|r| {
            Some(Score {
                name: r["name"].as_str().map(clean_name).unwrap_or_default(),
                mode: r["mode"].as_str()?.to_owned(),
                score: r["score"].as_f64().filter(|v| v.is_finite())?,
                shift: r["shift"].as_f64().unwrap_or(0.0) as i64,
                at: r["at"].as_f64().unwrap_or(0.0),
            })
        })
        .collect()
}

pub fn board_to_json(rows: &[Score]) -> String {
    Value::Array(
        rows.iter()
            .map(|r| json!({ "name": r.name, "mode": r.mode, "score": r.score, "shift": r.shift, "at": r.at }))
            .collect(),
    )
    .to_string()
}

/// saveLocalScore: add a row, then keep the top 20 per difficulty and for
/// the three most recent daily challenges.
pub fn add_score(rows: &mut Vec<Score>, s: Score) {
    rows.push(s);
    let mut days: Vec<String> = rows
        .iter()
        .map(|r| r.mode.clone())
        .filter(|m| is_daily_key(m))
        .collect();
    days.sort();
    days.dedup();
    let days = days[days.len().saturating_sub(3)..].to_vec();
    let mut kept = Vec::new();
    for m in ["relaxed", "standard", "hard"]
        .iter()
        .map(|m| (*m).to_owned())
        .chain(days)
    {
        let mut these: Vec<Score> = rows.iter().filter(|r| r.mode == m).cloned().collect();
        these.sort_by(|a, b| b.score.total_cmp(&a.score));
        these.truncate(BOARD_SIZE);
        kept.extend(these);
    }
    *rows = kept;
}

fn is_daily_key(m: &str) -> bool {
    let Some(d) = m.strip_prefix("daily-") else {
        return false;
    };
    let b = d.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

/// The board for one mode key, best first.
pub fn board_for(rows: &[Score], mode: &str) -> Vec<Score> {
    let mut v: Vec<Score> = rows.iter().filter(|r| r.mode == mode).cloned().collect();
    v.sort_by(|a, b| b.score.total_cmp(&a.score));
    v.truncate(BOARD_SIZE);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fighting_tallies_add_without_touching_career_stats() {
        let mut p = Profile::from_json(None);
        let stats = Stats {
            deaths: 1,
            jumpers: 3,
            ..Stats::default()
        };
        let kills = Kills {
            skiff: 2,
            boss: 1,
            ..Kills::default()
        };
        p.add_fighting(&stats, &kills);
        p.add_fighting(&stats, &kills);
        let stat = |k: &str| p.stats[PROFILE_STATS.iter().position(|s| s.0 == k).unwrap()];
        assert_eq!(
            (stat("deaths"), stat("kills"), stat("bosses")),
            (2.0, 6.0, 2.0)
        );
        assert_eq!((stat("shifts"), stat("banked")), (0.0, 0.0));
        assert_eq!(p.kills[0], 4);
        assert!(p.got.is_empty());
    }

    #[test]
    fn profile_load_validates_and_round_trips() {
        let p = Profile::from_json(Some(
            r#"{"stats":{"shifts":4,"banked":-5,"best":"x"},"got":{"vault":12,"bogus":1},
                "paint":"Gold!","kills":{"skiff":2.7,"jaws":-1}}"#,
        ));
        assert_eq!(p.stats[0], 4.0);
        assert_eq!(p.stats[1], 0.0, "negative stats are dropped");
        assert!(p.has("vault") && !p.has("bogus"));
        assert_eq!(p.paint, "class", "a malformed paint id falls back");
        assert_eq!(p.kills[0], 2);
        assert_eq!(p.kills[10], 0);
        assert_eq!(Profile::from_json(Some(&p.to_json())), p);
    }

    #[test]
    fn unlocks_once_and_paint_needs_its_achievement() {
        let mut p = Profile::from_json(None);
        p.paint = "gold".to_owned();
        assert_eq!(p.paint_colours(), None);
        assert_eq!(p.unlock("vault", 1.0), Some("Safecracker"));
        assert_eq!(p.unlock("vault", 2.0), None);
        assert_eq!(p.paint_colours(), Some((0x2c2408, 0xffe066)));
    }

    #[test]
    fn names_are_cleaned_like_the_browser() {
        assert_eq!(clean_name("  Ace <script>  Pilot!! "), "Ace script Pilot");
        assert_eq!(
            clean_name("O'Neil_the-2nd.Long-Name-Here"),
            "O'Neil_the-2nd.L"
        );
        assert_eq!(clean_name("!!!"), "");
    }

    #[test]
    fn the_board_keeps_twenty_per_mode_and_three_dailies() {
        let mut rows = Vec::new();
        for i in 0..25 {
            add_score(
                &mut rows,
                Score {
                    name: "A".into(),
                    mode: "hard".into(),
                    score: f64::from(i),
                    shift: 1,
                    at: 0.0,
                },
            );
        }
        for d in ["2026-01-01", "2026-01-02", "2026-01-03", "2026-01-04"] {
            add_score(
                &mut rows,
                Score {
                    name: "A".into(),
                    mode: format!("daily-{d}"),
                    score: 5.0,
                    shift: 1,
                    at: 0.0,
                },
            );
        }
        let hard = board_for(&rows, "hard");
        assert_eq!(hard.len(), 20);
        assert_eq!(hard[0].score, 24.0);
        assert!(
            board_for(&rows, "daily-2026-01-01").is_empty(),
            "only the last three days"
        );
        assert_eq!(board_for(&rows, "daily-2026-01-04").len(), 1);
        assert_eq!(board_from_json(Some(&board_to_json(&rows))), rows);
    }
}
