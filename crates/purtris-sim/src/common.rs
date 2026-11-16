//! Shared types: input bits, events, attack packets, styles and profiles.

/// InputBits (u16). Bit order is part of the replay format.
pub mod bits {
    pub const L: u16 = 1 << 0;
    pub const R: u16 = 1 << 1;
    pub const U: u16 = 1 << 2;
    pub const D: u16 = 1 << 3;
    pub const SD: u16 = 1 << 4;
    pub const HD: u16 = 1 << 5;
    pub const CW: u16 = 1 << 6;
    pub const CCW: u16 = 1 << 7;
    pub const R180: u16 = 1 << 8;
    pub const HOLD: u16 = 1 << 9;
    pub const SWAP: u16 = 1 << 10;
    pub const RAISE: u16 = 1 << 11;
    pub const TNEXT: u16 = 1 << 12;
    pub const TPREV: u16 = 1 << 13;
    /// Bit 14 was "reserved" in the design. It fixes an oversight: one bit per tick cannot
    /// express ARR 0 (instant shift) or an infinite soft-drop factor for modern Quad.
    /// FAST + L/R = shift to wall. FAST + SD = sonic drop. Only modern Quad reads it.
    pub const FAST: u16 = 1 << 14;
    /// Bit 15 stays reserved and must be 0.
    pub const RESERVED: u16 = 1 << 15;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Style {
    Quad,
    Chain,
    Capsule,
    Panel,
}
impl Style {
    pub const ALL: [Style; 4] = [Style::Quad, Style::Chain, Style::Capsule, Style::Panel];
    pub fn name(self) -> &'static str {
        match self {
            Style::Quad => "QUAD",
            Style::Chain => "CHAIN",
            Style::Capsule => "CAPSULE",
            Style::Panel => "PANEL",
        }
    }
    pub fn subtitle(self) -> &'static str {
        match self {
            Style::Quad => "stack falling blocks, clear lines",
            Style::Chain => "connect four blobs, build chains",
            Style::Capsule => "clear the germs, four in a row",
            Style::Panel => "Panel de Pon / Tetris Attack",
        }
    }
    pub fn default_profile(self) -> Profile {
        match self {
            Style::Quad => Profile::QuadPpt,
            Style::Chain => Profile::ChainTsu,
            Style::Capsule => Profile::CapsuleVs,
            Style::Panel => Profile::PanelPdp,
        }
    }
    pub fn index(self) -> usize {
        match self {
            Style::Quad => 0,
            Style::Chain => 1,
            Style::Capsule => 2,
            Style::Panel => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Profile {
    QuadPpt,
    QuadGuideline,
    ChainTsu,
    CapsuleVs,
    CapsuleNes,
    PanelPdp,
}
impl Profile {
    pub const ALL: [Profile; 6] = [
        Profile::QuadPpt,
        Profile::QuadGuideline,
        Profile::ChainTsu,
        Profile::CapsuleVs,
        Profile::CapsuleNes,
        Profile::PanelPdp,
    ];
    pub fn style(self) -> Style {
        match self {
            Profile::QuadPpt | Profile::QuadGuideline => Style::Quad,
            Profile::ChainTsu => Style::Chain,
            Profile::CapsuleVs | Profile::CapsuleNes => Style::Capsule,
            Profile::PanelPdp => Style::Panel,
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            Profile::QuadPpt => "quad.ppt",
            Profile::QuadGuideline => "quad.guideline",
            Profile::ChainTsu => "chain.tsu",
            Profile::CapsuleVs => "capsule.vs",
            Profile::CapsuleNes => "capsule.nes",
            Profile::PanelPdp => "panel.pdp",
        }
    }
    pub fn from_id(s: &str) -> Option<Profile> {
        Profile::ALL.iter().copied().find(|p| p.id() == s)
    }
    pub fn code(self) -> u8 {
        Profile::ALL.iter().position(|&p| p == self).unwrap() as u8
    }
    pub fn from_code(c: u8) -> Profile {
        Profile::ALL.get(c as usize).copied().unwrap_or(Profile::QuadPpt)
    }
}

/// An attack in the attacker's native units.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Native {
    Lines(u32),
    Nuisance(u32),
    Halves(u32),
    /// Panel garbage rectangles: (width, height).
    Rects(Vec<(u8, u8)>),
}
impl Native {
    /// Units used for cancel/offset. Panel never cancels.
    pub fn units(&self) -> u32 {
        match self {
            Native::Lines(n) | Native::Nuisance(n) | Native::Halves(n) => *n,
            Native::Rects(r) => r.iter().map(|&(w, h)| w as u32 * h as u32).sum(),
        }
    }
    pub fn is_zero(&self) -> bool {
        self.units() == 0
    }
    pub fn with_units(&self, n: u32) -> Native {
        match self {
            Native::Lines(_) => Native::Lines(n),
            Native::Nuisance(_) => Native::Nuisance(n),
            Native::Halves(_) => Native::Halves(n),
            Native::Rects(r) => Native::Rects(r.clone()),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Packet {
    pub native: Native,
    /// Common currency, cross path only.
    pub gauge: u32,
}

/// Public sim events. Audio, particles, stats and the analyzer read these.
#[derive(Clone, Debug)]
pub enum Ev {
    Move,
    Rotate,
    Lock { x: i32, y: i32 },
    HardDrop,
    Hold,
    /// A clear. power drives particles/sound; text is the callout.
    Clear { power: u32, text: String },
    ChainStep { n: u32 },
    AllClear,
    /// Garbage landed on this board.
    GarbageLand { n: u32 },
    /// Cell burst for particles: board cell coordinates and a color index.
    Burst { x: i32, y: i32, color: u8 },
    Swap,
    Rise,
    TopOut,
    VirusClear,
    LevelClear,
    Danger,
    AttackSent { units: u32 },
    Ko { victim: u8 },
}

#[derive(Default)]
pub struct Out {
    pub events: Vec<Ev>,
    pub attacks: Vec<Packet>,
}

/// Gauge to nuisance (design doc table, 1..=30).
pub const GAUGE_NUISANCE: [u32; 31] = [
    0, 4, 5, 6, 8, 10, 13, 16, 20, 24, 28, 33, 38, 43, 49, 55, 61, 68, 75, 83, 92, 102, 113, 125, 138,
    152, 167, 183, 200, 218, 237,
];

/// Every number that is a tuning pass rather than a capture. Shown in the room as "provisional".
pub const PROVISIONAL: &[(&str, &str)] = &[
    ("quad.combo", "Quad combo column is a tuning pass, not yet transcribed"),
    ("quad.holes", "Quad garbage hole: one uniform column per packet"),
    ("quad.timing", "Quad lock delay 30, 15 resets, ARE 6, clear 18 ticks"),
    ("chain.timing", "Chain fall/pop/lock tick counts"),
    ("chain.cross", "Chain export: 6 nuisance = 1 gauge (cross.chain.v1)"),
    ("capsule.vs", "Capsule versus table uses cross.capsule.v1 until Wii/3DS capture"),
    ("capsule.timing", "Capsule speeds, DAS 16/6, clear and fall delays"),
    ("panel.garbage", "Panel garbage chart (combo/chain rectangles)"),
    ("panel.timing", "Panel rise, flash, pop, hover, stop-time and cursor repeat"),
    ("cross.panel", "Panel export: 3 cells = 1 gauge (cross.panel.v1)"),
];
