//! The cast and the story. Characters are the 3D renders in assets/cast (see assets.rs); this
//! file gives them names, roles and lines. Story scenes play between story nodes.

pub struct Member {
    pub id: &'static str,
    pub name: &'static str,
    pub title: &'static str,
    pub bio: &'static str,
    /// favourite style: 0 Quad, 1 Chain, 2 Capsule, 3 Panel
    pub style: usize,
    pub color: u32,
}

pub const CAST: &[Member] = &[
    Member { id: "mira", name: "Mira", title: "Puzzle courier", bio: "Carries the Commons' last puzzle box from world to world. Never met a stack she couldn't flatten.", style: 0, color: 0xFFFF_9FCF },
    Member { id: "kai", name: "Kai", title: "Mira's sparring partner", bio: "Plays every style badly on purpose so Mira has someone to beat. Secretly very good at Chain.", style: 1, color: 0xFF7A_A8FF },
    Member { id: "nami", name: "Nami", title: "Lighthouse keeper of Quad Reef", bio: "Keeps the reef's lights burning and its rows perfectly clear. Thinks blocks should only ever fall straight down.", style: 0, color: 0xFF4F_D8E8 },
    Member { id: "hana", name: "Hana", title: "Gardener of Bloom Valley", bio: "Grows chains the way other people grow tomatoes: slowly, carefully, then all at once.", style: 1, color: 0xFF7B_D96A },
    Member { id: "yuki", name: "Yuki", title: "Night doctor of Clinic Moon", bio: "Treats every germ personally. Prescribes capsules in matching colors only.", style: 2, color: 0xFFF2_A6D6 },
    Member { id: "rin", name: "Rin", title: "Tile merchant of the Bazaar", bio: "Will swap you anything for anything. Her panels never stop rising, and neither do her prices.", style: 3, color: 0xFFF2_B640 },
    Member { id: "kaede", name: "Kaede", title: "Queen of Cinder Crown", bio: "Split the four styles into four worlds so no rule would ever bend. Undefeated, and tired of it.", style: 0, color: 0xFFE0_4458 },
    Member { id: "sora", name: "Sora", title: "Arcade regular", bio: "Holds the Bazaar's sprint record and will tell you about it.", style: 0, color: 0xFF6E_C8FF },
    Member { id: "ren", name: "Ren", title: "Arcade regular", bio: "Builds shrines out of Panel garbage. Calm until the stack reaches row eleven.", style: 3, color: 0xFFC0_8A5C },
    Member { id: "taro", name: "Taro", title: "Arcade regular", bio: "Practises Capsule on the night train home. Has cleared level 20 twice, once awake.", style: 2, color: 0xFF4F_B48C },
    Member { id: "emi", name: "Emi", title: "Arcade regular", bio: "Chains first, asks questions later.", style: 1, color: 0xFFFF_A64C },
];

pub fn get(id: &str) -> &'static Member {
    CAST.iter().find(|m| m.id == id).unwrap_or(&CAST[0])
}
pub fn index(id: &str) -> usize {
    CAST.iter().position(|m| m.id == id).unwrap_or(0)
}

// ------------------------------------------------------------------ story scenes

pub struct Line {
    pub who: &'static str,
    pub expr: &'static str,
    pub text: &'static str,
}

pub struct Scene {
    pub id: &'static str,
    /// the world map behind the scene
    pub world: usize,
    /// speakers shown on the right (everyone else stands on the left)
    pub right: &'static [&'static str],
    pub lines: &'static [Line],
}

macro_rules! l {
    ($w:expr, $e:expr, $t:expr) => {
        Line { who: $w, expr: $e, text: $t }
    };
}

pub const SCENES: &[Scene] = &[
    Scene {
        id: "prologue",
        world: 0,
        right: &["kai"],
        lines: &[
            l!("mira", "normal", "Once a year, everyone used to meet at the Commons and play every kind of puzzle on one big board."),
            l!("mira", "sad", "Then Queen Kaede split the styles into five worlds. Quad here, Chain there. No mixing, ever."),
            l!("kai", "smile", "So we go and ask nicely?"),
            l!("mira", "delighted", "We go and win nicely! Beat each world's champion at their own game, and they'll lend us their style."),
            l!("mira", "smile", "First stop, Quad Reef. Kai, grab the puzzle box."),
        ],
    },
    Scene {
        id: "w0_intro",
        world: 0,
        right: &["nami"],
        lines: &[
            l!("nami", "normal", "Visitors? The reef is closed to anything that doesn't fall in fours."),
            l!("mira", "smile", "Good thing I brought fours, then."),
            l!("kai", "shocked", "She sounds serious."),
            l!("nami", "smug", "Clear twenty rows on my shore before I clear mine, and we'll talk."),
        ],
    },
    Scene {
        id: "w0_boss",
        world: 0,
        right: &["nami"],
        lines: &[
            l!("nami", "angry", "You've been reading the lighthouse beam. Fine. No more lessons: a real match."),
            l!("mira", "delighted", "Finally!"),
        ],
    },
    Scene {
        id: "w0_clear",
        world: 0,
        right: &["nami"],
        lines: &[
            l!("nami", "sad", "My perfect rows... and you didn't even hold the I-piece."),
            l!("nami", "smile", "Take the reef's style. Maybe it's lonely out here anyway."),
            l!("kai", "smile", "Next: Bloom Valley. Bring snacks, Hana's chains take a while."),
        ],
    },
    Scene {
        id: "w1_intro",
        world: 1,
        right: &["hana"],
        lines: &[
            l!("hana", "smile", "Shh. The blobs are sleeping. When four of a color touch, they pop."),
            l!("hana", "smug", "And when the ones above them land on their friends... that's a chain."),
            l!("mira", "shocked", "That's... very many blobs."),
        ],
    },
    Scene {
        id: "w1_boss",
        world: 1,
        right: &["hana"],
        lines: &[
            l!("hana", "normal", "Patience grows the longest chains. Let's see how patient a courier can be."),
            l!("mira", "smile", "I brought snacks. I can wait all day."),
        ],
    },
    Scene {
        id: "w1_clear",
        world: 1,
        right: &["hana"],
        lines: &[
            l!("hana", "delighted", "Seven links! You planted that from the very first pair, didn't you?"),
            l!("hana", "smile", "The valley lends you its chains. Say hello to the moon for me."),
        ],
    },
    Scene {
        id: "w2_intro",
        world: 2,
        right: &["yuki"],
        lines: &[
            l!("yuki", "normal", "Welcome to Clinic Moon. Please disinfect your capsules."),
            l!("yuki", "smug", "Line up four of a color on the germs. Germs don't fall. Patients do."),
            l!("kai", "shocked", "That last part sounded like a threat."),
        ],
    },
    Scene {
        id: "w2_boss",
        world: 2,
        right: &["yuki"],
        lines: &[
            l!("yuki", "angry", "The Queen pays my clinic's light bill. I'm afraid I can't lose."),
            l!("mira", "smile", "Then I'm afraid it's going to be dark in here."),
        ],
    },
    Scene {
        id: "w2_clear",
        world: 2,
        right: &["yuki"],
        lines: &[
            l!("yuki", "sad", "Bottle clean. Not one germ left. Doctor's orders: I lose."),
            l!("yuki", "smile", "Kaede wasn't always like this, you know. She used to play everything."),
        ],
    },
    Scene {
        id: "w3_intro",
        world: 3,
        right: &["rin"],
        lines: &[
            l!("rin", "delighted", "Customers! Panels, fresh panels! Swap two, match three, everything must go!"),
            l!("rin", "smug", "The stack rises while we talk. Hope you swap faster than you haggle."),
            l!("kai", "smug", "We're not buying. We're winning."),
        ],
    },
    Scene {
        id: "w3_boss",
        world: 3,
        right: &["rin"],
        lines: &[
            l!("rin", "normal", "Last stall before the Crown. Win, and I'll throw in the Panel style for free."),
        ],
    },
    Scene {
        id: "w3_clear",
        world: 3,
        right: &["rin"],
        lines: &[
            l!("rin", "sad", "Sold out. Completely sold out."),
            l!("rin", "smile", "Go on, take the panels. Kaede's at the top of Cinder Crown. Mind the lava."),
            l!("kai", "shocked", "Lava?!"),
        ],
    },
    Scene {
        id: "w4_intro",
        world: 4,
        right: &["kaede"],
        lines: &[
            l!("kaede", "normal", "A courier with four borrowed styles. How charming."),
            l!("kaede", "angry", "Each game is perfect alone. Mix them and the rules bend. I won't allow it."),
            l!("mira", "angry", "They're better together. I'll show you on your own board."),
        ],
    },
    Scene {
        id: "w4_boss",
        world: 4,
        right: &["kaede"],
        lines: &[
            l!("kaede", "smug", "Every style at once, then. Let's see your Commons survive the Crown."),
            l!("kai", "smile", "Go get her, Mira."),
        ],
    },
    Scene {
        id: "w4_clear",
        world: 4,
        right: &["kaede"],
        lines: &[
            l!("kaede", "shocked", "My blocks... turned into your blobs... and it was fun."),
            l!("kaede", "sad", "I split the worlds because I kept losing at the Commons. Every game I wasn't best at."),
            l!("mira", "smile", "Everyone loses at something. That's why there are four games."),
            l!("kaede", "smile", "Then the Commons reopen. Tonight. Bring everyone."),
            l!("kai", "delighted", "Best news all year."),
        ],
    },
    Scene {
        id: "epilogue",
        world: 0,
        right: &["kaede", "nami", "hana", "yuki", "rin"],
        lines: &[
            l!("nami", "smile", "Six boards on one screen? The reef would never."),
            l!("hana", "delighted", "Chains landing on your Quad stack as rocks! It's beautiful!"),
            l!("rin", "smug", "Panel garbage for everyone. On the house."),
            l!("yuki", "smile", "I'll keep the clinic open late."),
            l!("kaede", "smile", "Same time next year, courier. I'll be practising Chain."),
            l!("mira", "delighted", "The Commons are open! Thanks for playing."),
        ],
    },
];

pub fn scene(id: &str) -> Option<&'static Scene> {
    SCENES.iter().find(|s| s.id == id)
}
