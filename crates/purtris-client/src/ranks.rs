//! Player levels, prestige and leaderboards. Everything is kept on this machine
//! (`ranks.txt` in the save folder) and never enters a match or a replay.

use crate::save::data_dir;

pub const MAX_LEVEL: u32 = 50;
pub const MAX_PRESTIGE: u32 = 10;

/// XP needed to go from `level` to `level + 1`.
pub fn xp_to_next(level: u32) -> u64 {
    200 + 40 * level as u64
}

pub const TITLES: [(u32, &str); 11] = [
    (1, "Newcomer"),
    (5, "Rookie"),
    (10, "Stacker"),
    (15, "Chain Starter"),
    (20, "Clinic Regular"),
    (25, "Bazaar Haggler"),
    (30, "Reef Navigator"),
    (35, "Bloom Keeper"),
    (40, "Crown Challenger"),
    (45, "Commons Ace"),
    (50, "Commons Legend"),
];
pub fn title(level: u32) -> &'static str {
    TITLES.iter().rev().find(|(l, _)| level >= *l).map(|(_, t)| *t).unwrap_or("Newcomer")
}

/// Prestige tiers: name and badge colour.
pub const PRESTIGE: [(&str, u32); 11] = [
    ("", 0xFF8C_93A8),
    ("Bronze", 0xFFCD_8A4F),
    ("Silver", 0xFFC8_D2E2),
    ("Gold", 0xFFF5_C84A),
    ("Platinum", 0xFF9F_E8E0),
    ("Diamond", 0xFF8E_C8FF),
    ("Ruby", 0xFFFF_5A7A),
    ("Sapphire", 0xFF4F_7BFF),
    ("Emerald", 0xFF3F_D98A),
    ("Starlight", 0xFFE6_B8FF),
    ("Commons Crown", 0xFFFF_9FCF),
];

/// The leaderboards. `lower` means a smaller value ranks higher (times).
pub struct Board {
    pub id: &'static str,
    pub name: &'static str,
    pub unit: &'static str,
    pub lower: bool,
}
pub const BOARDS: [Board; 9] = [
    Board { id: "QuadSprint", name: "Quad Sprint", unit: "time", lower: true },
    Board { id: "QuadTwenty", name: "Quad Twenty", unit: "time", lower: true },
    Board { id: "QuadMarathon", name: "Quad Marathon", unit: "score", lower: false },
    Board { id: "ChainEndless", name: "Chain Endless", unit: "score", lower: false },
    Board { id: "CapsuleLevelRun", name: "Capsule Level Run", unit: "score", lower: false },
    Board { id: "PanelEndless", name: "Panel Endless", unit: "score", lower: false },
    Board { id: "Arcade", name: "Arcade Ladder", unit: "stage", lower: false },
    Board { id: "VersusWins", name: "Versus Victories", unit: "wins", lower: false },
    Board { id: "TotalXP", name: "Lifetime XP", unit: "xp", lower: false },
];

#[derive(Clone, Debug)]
pub struct Entry {
    pub value: u64,
    pub name: String,
    pub character: String,
    pub level: u32,
    pub prestige: u32,
    pub when: u64,
}

#[derive(Clone, Default)]
pub struct Ranks {
    pub level: u32,
    /// XP inside the current level
    pub xp: u64,
    pub prestige: u32,
    /// every XP ever earned (all prestiges)
    pub lifetime: u64,
    pub boards: std::collections::BTreeMap<String, Vec<Entry>>,
}

/// What one match earned, for the results screen.
#[derive(Clone, Default)]
pub struct Gain {
    pub xp: u64,
    pub reasons: Vec<(String, u64)>,
    pub levels_gained: u32,
    /// (board name, rank 1-based) for any board the match placed on
    pub placed: Vec<(String, usize)>,
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl Ranks {
    pub fn load() -> Ranks {
        let mut r = Ranks { level: 1, ..Default::default() };
        let Ok(text) = std::fs::read_to_string(data_dir().join("ranks.txt")) else { return r };
        for line in text.lines() {
            let p: Vec<&str> = line.split('|').collect();
            match p.as_slice() {
                ["level", v] => r.level = v.parse().unwrap_or(1).clamp(1, MAX_LEVEL),
                ["xp", v] => r.xp = v.parse().unwrap_or(0),
                ["prestige", v] => r.prestige = v.parse().unwrap_or(0).min(MAX_PRESTIGE),
                ["lifetime", v] => r.lifetime = v.parse().unwrap_or(0),
                ["entry", board, value, name, ch, lv, pr, when] => {
                    r.boards.entry(board.to_string()).or_default().push(Entry {
                        value: value.parse().unwrap_or(0),
                        name: name.to_string(),
                        character: ch.to_string(),
                        level: lv.parse().unwrap_or(1),
                        prestige: pr.parse().unwrap_or(0),
                        when: when.parse().unwrap_or(0),
                    });
                }
                _ => {}
            }
        }
        r
    }

    pub fn save(&self) {
        let mut s = format!("level|{}\nxp|{}\nprestige|{}\nlifetime|{}\n", self.level, self.xp, self.prestige, self.lifetime);
        for (b, list) in &self.boards {
            for e in list {
                let clean = |x: &str| x.replace('|', "/").replace('\n', " ");
                s.push_str(&format!("entry|{}|{}|{}|{}|{}|{}|{}\n", b, e.value, clean(&e.name), clean(&e.character), e.level, e.prestige, e.when));
            }
        }
        let _ = std::fs::write(data_dir().join("ranks.txt"), s);
    }

    pub fn can_prestige(&self) -> bool {
        self.level >= MAX_LEVEL && self.prestige < MAX_PRESTIGE
    }

    /// Reset to level 1 one prestige higher.
    pub fn do_prestige(&mut self) {
        if self.can_prestige() {
            self.prestige += 1;
            self.level = 1;
            self.xp = 0;
            self.save();
        }
    }

    /// Add XP (with the prestige bonus: +5% per tier). Returns levels gained.
    pub fn add_xp(&mut self, base: u64) -> (u64, u32) {
        let xp = base + base * self.prestige as u64 * 5 / 100;
        self.lifetime += xp;
        let mut gained = 0;
        if self.level >= MAX_LEVEL {
            return (xp, 0);
        }
        self.xp += xp;
        while self.level < MAX_LEVEL && self.xp >= xp_to_next(self.level) {
            self.xp -= xp_to_next(self.level);
            self.level += 1;
            gained += 1;
        }
        if self.level >= MAX_LEVEL {
            self.xp = 0;
        }
        (xp, gained)
    }

    /// Submit a score; returns the rank it took (1-based) if it made the top 10.
    pub fn submit(&mut self, board: &str, value: u64, name: &str, character: &str) -> Option<usize> {
        let lower = BOARDS.iter().find(|b| b.id == board).map(|b| b.lower).unwrap_or(false);
        let e = Entry { value, name: name.to_string(), character: character.to_string(), level: self.level, prestige: self.prestige, when: now() };
        let list = self.boards.entry(board.to_string()).or_default();
        list.push(e);
        list.sort_by(|a, b| if lower { a.value.cmp(&b.value) } else { b.value.cmp(&a.value) }.then(a.when.cmp(&b.when)));
        let pos = list.iter().position(|x| x.value == value && x.name == name && x.when >= now().saturating_sub(2));
        list.truncate(10);
        pos.filter(|&p| p < 10).map(|p| p + 1)
    }

    /// One entry per player name that only ever grows (versus wins, lifetime XP).
    pub fn bump(&mut self, board: &str, name: &str, character: &str, by: u64, absolute: bool) -> Option<usize> {
        let (level, prestige) = (self.level, self.prestige);
        let list = self.boards.entry(board.to_string()).or_default();
        if let Some(e) = list.iter_mut().find(|e| e.name == name) {
            e.value = if absolute { by } else { e.value + by };
            e.character = character.to_string();
            e.level = level;
            e.prestige = prestige;
            e.when = now();
        } else {
            list.push(Entry { value: by, name: name.to_string(), character: character.to_string(), level, prestige, when: now() });
        }
        list.sort_by(|a, b| b.value.cmp(&a.value));
        list.truncate(10);
        list.iter().position(|e| e.name == name).map(|p| p + 1)
    }
}

pub fn fmt_value(board: &Board, v: u64) -> String {
    match board.unit {
        "time" => crate::render::fmt_time(v as u32),
        "stage" => format!("Stage {}", v),
        "wins" => format!("{} wins", v),
        "xp" => format!("{} XP", v),
        _ => format!("{}", v),
    }
}
