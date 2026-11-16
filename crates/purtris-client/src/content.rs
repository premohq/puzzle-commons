//! Original content: the Story overworld (5 worlds x 6 nodes) and the training curriculum.
//! Every level is a MatchHeader the sim already accepts plus objectives scored afterward.

use purtris_sim::{MatchHeader, Mode, Opts, Personality, Profile, SeatCfg, SeatKind, Style, TargetRule};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Obj {
    Win,
    /// finish at or before (normal, hard) ticks
    TickPar(u32, u32),
    SendAtLeast(u32, u32),
    ChainAtLeast(u32, u32),
    ClearViruses,
    TeamTwoAlive,
    SurviveTicks(u32, u32),
}
impl Obj {
    pub fn text(self, diff: usize) -> String {
        let pick = |n: u32, h: u32| if diff >= 2 { h } else { n };
        match self {
            Obj::Win => "Win the match".into(),
            Obj::TickPar(n, h) => format!("Finish within {}", crate::render::fmt_time(pick(n, h))),
            Obj::SendAtLeast(n, h) => format!("Send at least {}", pick(n, h)),
            Obj::ChainAtLeast(n, h) => format!("Reach a {}-chain", pick(n, h)),
            Obj::ClearViruses => "Clear every germ".into(),
            Obj::TeamTwoAlive => "Finish with 2 teammates alive".into(),
            Obj::SurviveTicks(n, h) => format!("Survive {}", crate::render::fmt_time(pick(n, h))),
        }
    }
}

#[derive(Clone, Copy)]
pub struct Level {
    pub mode: Mode,
    /// None: the player picks a style on the map
    pub human: Option<Profile>,
    pub allies: &'static [Profile],
    pub foes: &'static [Profile],
    pub ffa: bool,
    pub boss: bool,
}

pub struct Node {
    pub name: &'static str,
    pub line: &'static str,
    pub level: Level,
    pub extra: Obj,
}

pub struct World {
    pub name: &'static str,
    pub style: &'static str,
    pub nodes: [Node; 6],
}

use Profile::*;
const Q: &[Profile] = &[QuadPpt];
const C: &[Profile] = &[ChainTsu];
const K: &[Profile] = &[CapsuleVs];
const P: &[Profile] = &[PanelPdp];
const NONE: &[Profile] = &[];

const fn duel(foes: &'static [Profile], human: Option<Profile>) -> Level {
    Level { mode: Mode::Versus, human, allies: NONE, foes, ffa: false, boss: false }
}
const fn boss(foes: &'static [Profile], human: Option<Profile>) -> Level {
    Level { mode: Mode::Versus, human, allies: NONE, foes, ffa: false, boss: true }
}
const fn team(allies: &'static [Profile], foes: &'static [Profile], human: Option<Profile>) -> Level {
    Level { mode: Mode::Versus, human, allies, foes, ffa: false, boss: false }
}
const fn solo(mode: Mode, p: Profile) -> Level {
    Level { mode, human: Some(p), allies: NONE, foes: NONE, ffa: false, boss: false }
}

pub const WORLDS: [World; 5] = [
    World {
        name: "Quad Reef",
        style: "QUAD",
        nodes: [
            Node { name: "Lantern Shore", line: "Hi, I'm Mira, courier! Stack the falling gems and fill rows to clear them. Twenty rows and this delivery's done.", level: solo(Mode::QuadTwenty, QuadGuideline), extra: Obj::TickPar(5400, 3000) },
            Node { name: "Coral Steps", line: "Clear two or more rows to send garbage to your rival. Four at once hits hard. Nami calls it a tidal wave, then blushes.", level: duel(Q, Some(QuadPpt)), extra: Obj::SendAtLeast(6, 10) },
            Node { name: "Bubble Rise", line: "Your clears cancel garbage still on its way. Watch the meter on the left, the way Nami watches the harbor.", level: duel(Q, Some(QuadPpt)), extra: Obj::SendAtLeast(8, 14) },
            Node { name: "Kelp Gate", line: "Hold a piece for later with the hold key. Plan one piece ahead. Nami gave her hold slot a name. Ask her.", level: duel(Q, Some(QuadPpt)), extra: Obj::TickPar(7200, 5400) },
            Node { name: "Twin Current", line: "Kai joins in! Two against two, last team standing wins. He says he's bad at this. We'll see.", level: team(Q, &[QuadPpt, QuadPpt], Some(QuadPpt)), extra: Obj::TeamTwoAlive },
            Node { name: "Reef Warden", line: "Nami stacks fast and sends often. Keep your stack low, hit back, and stand up straight. She'll check.", level: boss(Q, Some(QuadPpt)), extra: Obj::SendAtLeast(10, 18) },
        ],
    },
    World {
        name: "Bloom Valley",
        style: "CHAIN",
        nodes: [
            Node { name: "Seed Plot", line: "Join four blobs of one color and they pop. Keep the X cell at the top clear! Gerald fell in it once.", level: duel(C, Some(ChainTsu)), extra: Obj::ChainAtLeast(2, 3) },
            Node { name: "Sprout Path", line: "When one pop makes others fall and pop, that's a chain. Chains send far more. Hana says they bloom.", level: duel(C, Some(ChainTsu)), extra: Obj::ChainAtLeast(3, 4) },
            Node { name: "Rain Terrace", line: "A small chain can cancel nuisance in your tray before it lands. Like an umbrella, but angrier.", level: duel(C, Some(ChainTsu)), extra: Obj::SendAtLeast(10, 24) },
            Node { name: "Root Hollow", line: "Build stairs: groups of three stacked so each pop feeds the next. Hana's flowers climb old gray stone the same way.", level: duel(C, Some(ChainTsu)), extra: Obj::ChainAtLeast(4, 5) },
            Node { name: "Pollen Pair", line: "Two blob builders on your side. Fire when your partner's in trouble. Kai is in trouble a lot. Suspiciously.", level: team(C, &[ChainTsu, ChainTsu], Some(ChainTsu)), extra: Obj::TeamTwoAlive },
            Node { name: "Bloom Keeper", line: "Hana builds tall and fires big. Fire first, or fire bigger. Then tea. Gerald insists.", level: boss(C, Some(ChainTsu)), extra: Obj::ChainAtLeast(4, 6) },
        ],
    },
    World {
        name: "Clinic Moon",
        style: "CAPSULE",
        nodes: [
            Node { name: "Germ Garden", line: "Line up four of one color, capsule halves or germs, to clear them. Clear every germ first to win! Yuki says wash up.", level: Level { mode: Mode::CapsuleRace, human: Some(CapsuleVs), allies: NONE, foes: K, ffa: false, boss: false }, extra: Obj::TickPar(7200, 4800) },
            Node { name: "Vial Row", line: "In versus, two lines at once send loose halves to your rival. Yuki calls that \"sharing your medicine.\"", level: duel(K, Some(CapsuleVs)), extra: Obj::SendAtLeast(3, 6) },
            Node { name: "Pill Drop", line: "Halves that lose their partner fall. A fall that makes a line is a chain. Kai says he's allergic to chains. Sure.", level: duel(K, Some(CapsuleVs)), extra: Obj::ChainAtLeast(2, 3) },
            Node { name: "Quarantine", line: "Another race. Take the low germs first so nothing gets buried. I keep thinking about that quiet ward.", level: Level { mode: Mode::CapsuleRace, human: Some(CapsuleVs), allies: NONE, foes: K, ffa: false, boss: false }, extra: Obj::TickPar(6000, 4200) },
            Node { name: "Ward Pair", line: "Two on two in the clinic. Keep your bottle neck open! Kai's quiet today. Not even a fake cough.", level: team(K, &[CapsuleVs, CapsuleVs], Some(CapsuleVs)), extra: Obj::TeamTwoAlive },
            Node { name: "Head Nurse", line: "She clears fast and sends often. Clear germs and send back. Yuki's prognosis: we lose. Mine: we don't.", level: boss(K, Some(CapsuleVs)), extra: Obj::SendAtLeast(6, 10) },
        ],
    },
    World {
        name: "Tile Bazaar",
        style: "PANEL",
        nodes: [
            Node { name: "Charm Stall", line: "Panels don't fall from the sky here. Swap two side by side to make three in a row. Rin says swapping is free. Today.", level: solo(Mode::PanelPuzzle(1), PanelPdp), extra: Obj::Win },
            Node { name: "Swap Lane", line: "The stack keeps rising, like Rin's prices. Clear panels to pause it, and don't let it reach the top line.", level: duel(P, Some(PanelPdp)), extra: Obj::ChainAtLeast(2, 3) },
            Node { name: "Rising Market", line: "Garbage stones land on you. Clear panels next to them to crack them open. Gray again. I'm fine. Totally fine.", level: duel(P, Some(PanelPdp)), extra: Obj::SendAtLeast(6, 12) },
            Node { name: "Puzzle Kiosk", line: "Two moves, two rows. Think before you swap. Some people should think before they lie. Just saying.", level: solo(Mode::PanelPuzzle(4), PanelPdp), extra: Obj::Win },
            Node { name: "Lantern Pair", line: "Team panels! Chains crack stones and send them across. I need a partner I can trust. Kai keeps following me anyway. Ugh.", level: team(P, &[PanelPdp, PanelPdp], Some(PanelPdp)), extra: Obj::TeamTwoAlive },
            Node { name: "Bazaar Chief", line: "The Chief cracks stones and returns chains. Stay low, build quietly. Then Kai and I are talking.", level: boss(P, Some(PanelPdp)), extra: Obj::ChainAtLeast(3, 4) },
        ],
    },
    World {
        name: "Cinder Crown",
        style: "MIXED",
        nodes: [
            Node { name: "First Meeting", line: "Cinder Crown, where every style meets! Your quad clears land as nuisance blobs on their side. Rude, but effective.", level: duel(C, Some(QuadPpt)), extra: Obj::SendAtLeast(8, 14) },
            Node { name: "Your Way", line: "Pick any style you like. Your rival plays panels. Every style I borrowed came with a friend attached.", level: duel(P, None), extra: Obj::SendAtLeast(6, 12) },
            Node { name: "Mixed Pair", line: "A capsule medic fights beside you against blocks and blobs. Yuki's orders: \"Don't top out. Paperwork.\"", level: team(K, &[QuadPpt, ChainTsu], None), extra: Obj::TeamTwoAlive },
            Node { name: "Three by Three", line: "Six boards at once, so watch who is sending at you. Kaede sends at everyone. Mostly at herself.", level: team(&[ChainTsu, PanelPdp], &[QuadPpt, CapsuleVs, ChainTsu], None), extra: Obj::TeamTwoAlive },
            Node { name: "Free For All", line: "Three players, no teams, last one standing wins. After today, nobody has to stand alone.", level: Level { mode: Mode::Versus, human: None, allies: NONE, foes: &[QuadPpt, PanelPdp], ffa: true, boss: false }, extra: Obj::SendAtLeast(8, 16) },
            Node { name: "Grand Voyage", line: "The volcano is waking. Everything you've learned, all at once. Old boards remember. Let's finish this together!", level: Level { mode: Mode::Versus, human: None, allies: &[PanelPdp, CapsuleVs], foes: &[QuadPpt, ChainTsu, PanelPdp], ffa: false, boss: true }, extra: Obj::TeamTwoAlive },
        ],
    },
];

pub const DIFF_NAMES: [&str; 3] = ["EASY", "NORMAL", "HARD"];

/// Build the match header for a node.
pub fn node_header(world: usize, node: usize, diff: usize, chosen: Profile, seed: u64) -> MatchHeader {
    let n = &WORLDS[world].nodes[node];
    let lv = n.level;
    let mut seats = [SeatCfg::default(); 6];
    let human = lv.human.unwrap_or(chosen);
    let is_team = !lv.allies.is_empty() || lv.foes.len() > 1 && !lv.ffa;
    seats[0] = SeatCfg { kind: SeatKind::Human, profile: human, team: if is_team { 1 } else { 0 }, ..SeatCfg::default() };
    let (pers, strength) = match diff {
        0 => (Personality::Builder, 2),
        1 => (Personality::Builder, 4),
        _ => (Personality::Aggressor, 7),
    };
    let strength = if lv.boss { (strength + 1).min(8) } else { strength };
    let mut k = 1;
    for &p in lv.allies {
        let per = if p.style() == Style::Panel && diff == 0 { Personality::Cleaner } else { Personality::Builder };
        seats[k] = SeatCfg { kind: SeatKind::Bot, profile: p, team: 1, personality: per, strength: 4, bot_seed: 1000 + k as u32 };
        k += 1;
    }
    for &p in lv.foes {
        let per = if p.style() == Style::Panel && diff == 0 { Personality::Cleaner } else { pers };
        seats[k] = SeatCfg { kind: SeatKind::Bot, profile: p, team: if is_team { 2 } else { 0 }, personality: per, strength, bot_seed: 2000 + k as u32 };
        k += 1;
    }
    let mut opts = Opts::default();
    opts.capsule_level = [2, 5, 8][diff];
    opts.capsule_speed = [0, 1, 1][diff];
    opts.panel_level = [2, 3, 5][diff];
    opts.shared_deal = lv.mode == Mode::CapsuleRace;
    let target = if is_team { TargetRule::Team } else { TargetRule::Direct };
    MatchHeader { seed, mode: lv.mode, target, seats, opts }
}

// ------------------------------------------------------------------ curriculum

pub struct Lesson {
    pub id: &'static str,
    pub track: &'static str,
    pub tier: &'static str,
    pub title: &'static str,
    pub idea: &'static str,
    pub why: &'static str,
    pub fail: &'static str,
    pub review: &'static str,
    pub drill: Drill,
    pub obj: Obj,
}

#[derive(Clone, Copy)]
pub struct Drill {
    pub mode: Mode,
    pub profile: Profile,
    pub foe: Option<(Profile, Personality, u8)>,
    pub capsule_level: u32,
    pub capsule_speed: u8,
}
const fn d_solo(mode: Mode, profile: Profile) -> Drill {
    Drill { mode, profile, foe: None, capsule_level: 3, capsule_speed: 1 }
}
const fn d_vs(profile: Profile, foe: Profile, p: Personality, s: u8) -> Drill {
    Drill { mode: Mode::Versus, profile, foe: Some((foe, p, s)), capsule_level: 4, capsule_speed: 1 }
}

pub const TRACKS: [&str; 6] = ["METHOD", "QUAD", "CHAIN", "CAPSULE", "PANEL", "CROSS"];

pub const LESSONS: &[Lesson] = &[
    Lesson {
        id: "m1", track: "METHOD", tier: "FUNDAMENTALS", title: "Name the win condition",
        idea: "Before the first piece, say how this match is won: KO, race, score, or clear.",
        why: "A race player who builds attacks loses time; a KO player who ignores attacks loses the board. Drop the plan the moment danger arrives.",
        fail: "Build the prettiest stack you can and ignore the meter. Notice the red bar on the left: that is the attack you chose not to answer.",
        review: "After the match, find the moment the meter went red and write down what you placed next.",
        drill: d_vs(QuadPpt, ChainTsu, Personality::Builder, 2), obj: Obj::Win,
    },
    Lesson {
        id: "q1", track: "QUAD", tier: "FUNDAMENTALS", title: "A clean stack",
        idea: "Keep the surface flat and never cover an empty cell.",
        why: "Holes cost a line clear each to dig out. A flat surface fits every piece the bag can give you. Only leave the right column open on purpose.",
        fail: "Drop pieces wherever they land first. Count the holes after twenty lines.",
        review: "Look for the first hole you made and the piece you could have placed elsewhere.",
        drill: d_solo(Mode::QuadTwenty, QuadGuideline), obj: Obj::TickPar(4800, 3000),
    },
    Lesson {
        id: "q2", track: "QUAD", tier: "FUNDAMENTALS", title: "Hold and the bag",
        idea: "Every 7 pieces contain one of each shape. Hold saves a piece for later.",
        why: "Knowing the bag means you know an I piece is never more than 12 pieces away. Hold turns a bad piece into a good one, but only once per drop.",
        fail: "Never use hold. Notice how often an S or Z ruins a flat stack.",
        review: "Count how many times you held this game, and whether each hold made a cleaner placement.",
        drill: d_solo(Mode::QuadSprint, QuadGuideline), obj: Obj::TickPar(10800, 6000),
    },
    Lesson {
        id: "q3", track: "QUAD", tier: "INTERMEDIATE", title: "Finesse tapping",
        idea: "Reach any column with the fewest presses; let DAS carry you to the walls.",
        why: "Fewer presses means fewer misdrops and more pieces per second. This is finesse, not mashing: ARR can be 0 and one held press reaches the wall. Hypertapping is a console skill and is not the way to play this profile.",
        fail: "Tap once per column every time. Watch the analyzer count the extra presses.",
        review: "The analyzer lists pieces that used more than 4 presses. Replay one and find the shorter path.",
        drill: d_solo(Mode::QuadTwenty, QuadGuideline), obj: Obj::TickPar(3600, 2400),
    },
    Lesson {
        id: "q4", track: "QUAD", tier: "INTERMEDIATE", title: "T-spins",
        idea: "Rotate a T into a slot so three corners around its center are filled.",
        why: "A T-spin double sends 4 lines for only 2 cleared, and keeps back-to-back alive. Build the slot only when your stack is safe.",
        fail: "Clear singles all game. Compare what you sent to a T-spin player.",
        review: "Find a T piece you placed flat while a slot was one move away.",
        drill: d_vs(QuadPpt, QuadPpt, Personality::Builder, 3), obj: Obj::SendAtLeast(10, 16),
    },
    Lesson {
        id: "q5", track: "QUAD", tier: "ADVANCED", title: "Back-to-back and cancel timing",
        idea: "Chain quads and T-spins for +1 each; clear to cancel garbage before it lands.",
        why: "Incoming lines can be cancelled by your own clears while they travel. A clear on the right tick turns their attack into nothing.",
        fail: "Place pieces that don't clear while the meter is full, and eat the garbage.",
        review: "Find a time the orange bar turned red. Was a clear available one piece earlier?",
        drill: d_vs(QuadPpt, QuadPpt, Personality::Aggressor, 5), obj: Obj::SendAtLeast(15, 24),
    },
    Lesson {
        id: "q6", track: "QUAD", tier: "MASTER", title: "Hold the gauge for a Chain defender",
        idea: "Against Chain, your combo gauge is held until the combo ends, then converted together.",
        why: "Two quads released together convert to more nuisance than two quads sent apart, because the nuisance table grows faster than the gauge.",
        fail: "Break your combo after every clear against a Chain bot. Check what you sent.",
        review: "Compare 'sent' at the end with the lesson par.",
        drill: d_vs(QuadPpt, ChainTsu, Personality::Builder, 5), obj: Obj::Win,
    },
    Lesson {
        id: "c1", track: "CHAIN", tier: "FUNDAMENTALS", title: "Groups of four and the X",
        idea: "Four connected blobs of one color pop. A blob in the X cell ends your game.",
        why: "The X is the third column of the top row. Your next pair spawns there, so keep that column lower than the rest.",
        fail: "Stack straight up the middle. Watch how quickly the X fills.",
        review: "Note your tallest column at the end and where it was.",
        drill: d_solo(Mode::ChainEndless, ChainTsu), obj: Obj::ChainAtLeast(2, 3),
    },
    Lesson {
        id: "c2", track: "CHAIN", tier: "FUNDAMENTALS", title: "Your first 3-chain",
        idea: "Set a group of three on top of another group so one pop triggers the next.",
        why: "Each link multiplies the score, so a 3-chain sends far more than three single pops. Keep the trigger reachable.",
        fail: "Pop every group as soon as it forms. Count your nuisance sent.",
        review: "Find the blob that would have been your trigger.",
        drill: d_solo(Mode::ChainEndless, ChainTsu), obj: Obj::ChainAtLeast(3, 4),
    },
    Lesson {
        id: "c3", track: "CHAIN", tier: "INTERMEDIATE", title: "Stairs and leftover",
        idea: "Stairs stack groups diagonally; leftover score carries to the next chain in seventieths.",
        why: "The same chain can send a different amount next time because leftover carries. Margin time lowers the target and makes everything stronger.",
        fail: "Fire 2-chains repeatedly and watch the leftover never add up.",
        review: "Check the target points (TP) under your board at the end of a long match.",
        drill: d_solo(Mode::ChainEndless, ChainTsu), obj: Obj::ChainAtLeast(4, 5),
    },
    Lesson {
        id: "c4", track: "CHAIN", tier: "ADVANCED", title: "Offset with a harassment chain",
        idea: "A small chain cancels nuisance in your tray before it falls.",
        why: "Offset buys time for your main chain. Fire small only when the tray would hurt; otherwise keep building.",
        fail: "Ignore the tray against an aggressive bot and let nuisance bury your build.",
        review: "Find the moment the tray filled and whether a 1- or 2-chain was ready.",
        drill: d_vs(ChainTsu, ChainTsu, Personality::Aggressor, 4), obj: Obj::Win,
    },
    Lesson {
        id: "k1", track: "CAPSULE", tier: "FUNDAMENTALS", title: "Four, not three",
        idea: "A clear needs four in a straight line. Use the next capsule shown above the bottle.",
        why: "Three of a color is a setup, not a clear. Pre-rotate for the next capsule and keep a landing ready for it.",
        fail: "Stack capsules on germs of a different color. See how you strand them.",
        review: "Find a germ you covered with the wrong color.",
        drill: Drill { mode: Mode::CapsuleLevelRun, profile: CapsuleNes, foe: None, capsule_level: 2, capsule_speed: 0 }, obj: Obj::ClearViruses,
    },
    Lesson {
        id: "k2", track: "CAPSULE", tier: "INTERMEDIATE", title: "Fall chains",
        idea: "When a clear frees a half, it falls. If it lands in a line of four, that's a chain.",
        why: "In versus, each extra fall step adds to what you send. Plan a free half above a ready line.",
        fail: "Clear only what is in front of you and never set up a fall.",
        review: "Count your chain steps in the stats at the end.",
        drill: d_vs(CapsuleVs, CapsuleVs, Personality::Builder, 3), obj: Obj::ChainAtLeast(2, 3),
    },
    Lesson {
        id: "k3", track: "CAPSULE", tier: "ADVANCED", title: "Hypertap and rolling",
        idea: "On console handling, pressing faster than the auto-repeat moves the capsule sooner than holding.",
        why: "This profile's repeat is in-sim: 16 ticks to charge, then every 6 ticks (provisional, waiting on the capture file). A fresh press beats the repeat above 60/6 = 10 Hz. On keyboard, alternate two fingers on the same key; on a pad, roll your thumb.",
        fail: "Hold the direction at HI speed and watch the capsule miss the column.",
        review: "Did you reach the far wall before the capsule landed on HI?",
        drill: Drill { mode: Mode::CapsuleLevelRun, profile: CapsuleNes, foe: None, capsule_level: 4, capsule_speed: 2 }, obj: Obj::ClearViruses,
    },
    Lesson {
        id: "k4", track: "CAPSULE", tier: "MASTER", title: "Race versus send",
        idea: "In a race, garbage does nothing. Every move should take a germ.",
        why: "Capsule Race is the original two-player game: same bottle, first clear wins. Don't spend moves on attacks the mode ignores.",
        fail: "Build big combos in a race and lose on time.",
        review: "Compare your finish time to the par.",
        drill: Drill { mode: Mode::CapsuleRace, profile: CapsuleVs, foe: Some((CapsuleVs, Personality::Racer, 4)), capsule_level: 3, capsule_speed: 1 }, obj: Obj::Win,
    },
    Lesson {
        id: "p1", track: "PANEL", tier: "FUNDAMENTALS", title: "Swap and match",
        idea: "Swap the two panels under the cursor. Three in a line clears.",
        why: "There are no falling pieces: you make every match by moving panels sideways. Panels can also slide into empty space and drop.",
        fail: "Swap at random and run out of moves.",
        review: "How many swaps did you use compared with the moves allowed?",
        drill: d_solo(Mode::PanelPuzzle(0), PanelPdp), obj: Obj::Win,
    },
    Lesson {
        id: "p2", track: "PANEL", tier: "FUNDAMENTALS", title: "Raise and the top line",
        idea: "The stack rises. Raise it yourself when it is low; never let it reach the top line.",
        why: "Raising early gives you panels to work with. Clears pause the rise: that is stop-time, a budget you spend.",
        fail: "Never raise and wait for panels; then panic at the top.",
        review: "When did the danger line first flash?",
        drill: d_solo(Mode::PanelEndless, PanelPdp), obj: Obj::SurviveTicks(5400, 9000),
    },
    Lesson {
        id: "p3", track: "PANEL", tier: "INTERMEDIATE", title: "Chains from falling panels",
        idea: "A match made by panels falling after a clear is a chain.",
        why: "Chains send tall garbage and long stop-time. Set a panel above a gap so the next clear drops it into place.",
        fail: "Make only flat 3-matches. Compare your best chain.",
        review: "Your BEST x number shows your longest chain.",
        drill: d_solo(Mode::PanelPuzzle(2), PanelPdp), obj: Obj::Win,
    },
    Lesson {
        id: "p4", track: "PANEL", tier: "ADVANCED", title: "Crack the stone",
        idea: "Clear panels touching a garbage stone to crack it into new panels.",
        why: "The new panels can chain. Panel never cancels garbage: cracking it and chaining the result is the answer.",
        fail: "Leave stones on your stack and build on top of them.",
        review: "Count how many stones you cracked versus how many reached the top.",
        drill: d_vs(PanelPdp, PanelPdp, Personality::Punisher, 3), obj: Obj::SendAtLeast(6, 12),
    },
    Lesson {
        id: "p5", track: "PANEL", tier: "MASTER", title: "Combo versus chain",
        idea: "Several groups clearing in one step is a combo; one after another is a chain.",
        why: "Combos send wide garbage now. Chains send tall garbage later. Choose by what your rival can crack.",
        fail: "Solve with two separate single matches instead of one combo.",
        review: "Did your solution clear six at once?",
        drill: d_solo(Mode::PanelPuzzle(4), PanelPdp), obj: Obj::Win,
    },
    Lesson {
        id: "x1", track: "CROSS", tier: "READING", title: "Reading a Chain player",
        idea: "Watch the height of their chain, their trigger, their tray, and the margin clock.",
        why: "A tall build means a big chain is coming: send early to break it, or hold your gauge and prepare to cancel.",
        fail: "Never look at the Chain board. Get surprised by a 5-chain.",
        review: "When did their chain fire, and what were you doing?",
        drill: d_vs(QuadPpt, ChainTsu, Personality::Builder, 4), obj: Obj::Win,
    },
    Lesson {
        id: "x2", track: "CROSS", tier: "READING", title: "Reading a Panel player",
        idea: "Watch their stop-time, the stones on their stack, and whether they are cracking for a return chain.",
        why: "Panel can't cancel your attacks, but cracked stones become panels that chain back at you.",
        fail: "Send into a long stop-time and get a chain back.",
        review: "Note when their STOP bar was full and when you sent.",
        drill: d_vs(ChainTsu, PanelPdp, Personality::Punisher, 4), obj: Obj::Win,
    },
];

pub fn lesson_header(l: &Lesson, seed: u64, human: bool) -> MatchHeader {
    let d = l.drill;
    let mut seats = [SeatCfg::default(); 6];
    seats[0] = SeatCfg {
        kind: if human { SeatKind::Human } else { SeatKind::Bot },
        profile: d.profile,
        personality: Personality::Builder,
        strength: 6,
        bot_seed: 77,
        team: 0,
    };
    if let Some((p, per, s)) = d.foe {
        seats[1] = SeatCfg { kind: SeatKind::Bot, profile: p, team: 0, personality: per, strength: s, bot_seed: 99 };
    }
    let mut opts = Opts::default();
    opts.capsule_level = d.capsule_level;
    opts.capsule_speed = d.capsule_speed;
    opts.shared_deal = d.mode == Mode::CapsuleRace;
    MatchHeader { seed, mode: d.mode, target: TargetRule::Direct, seats, opts }
}
