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
    Member { id: "kai", name: "Kai", title: "Mira's oldest friend", bio: "Brings the worst snack every world sells. Loses by exactly one row, every time. Swears that's a coincidence.", style: 1, color: 0xFF7A_A8FF },
    Member { id: "nami", name: "Nami", title: "Lighthouse keeper of Quad Reef", bio: "Keeps the reef's lights burning and its rows perfectly clear. Thinks blocks should only ever fall straight down.", style: 0, color: 0xFF4F_D8E8 },
    Member { id: "hana", name: "Hana", title: "Gardener of Bloom Valley", bio: "Grows chains the way other people grow tomatoes: slowly, carefully, then all at once.", style: 1, color: 0xFF7B_D96A },
    Member { id: "yuki", name: "Yuki", title: "Night doctor of Clinic Moon", bio: "Treats every germ personally. Prescribes capsules in matching colors only.", style: 2, color: 0xFFF2_A6D6 },
    Member { id: "rin", name: "Rin", title: "Tile merchant of the Bazaar", bio: "Will swap you anything for anything. Her panels never stop rising, and neither do her prices.", style: 3, color: 0xFFF2_B640 },
    Member { id: "kaede", name: "Kaede", title: "Queen of Cinder Crown", bio: "Split the styles into five worlds and closed the Commons. Practises chains daily. Undefeated, and tired of it.", style: 0, color: 0xFFE0_4458 },
    Member { id: "sora", name: "Sora", title: "Arcade regular", bio: "Holds the Bazaar's sprint record and will tell you about it.", style: 0, color: 0xFF6E_C8FF },
    Member { id: "ren", name: "Ren", title: "Arcade regular", bio: "Builds shrines out of Panel garbage. Calm until the stack reaches row eleven.", style: 3, color: 0xFFC0_8A5C },
    Member { id: "taro", name: "Taro", title: "Arcade regular", bio: "Practises Capsule on the night train home. Has cleared level 20 twice, once awake.", style: 2, color: 0xFF4F_B48C },
    Member { id: "emi", name: "Emi", title: "Arcade regular", bio: "Chains first, asks questions later.", style: 1, color: 0xFFFF_A64C },
    Member { id: "aoi", name: "Aoi", title: "Night nurse of Clinic Moon", bio: "Yuki's assistant. Counts sheep for the patients so they don't have to.", style: 2, color: 0xFF2F_B7A8 },
    Member { id: "haru", name: "Haru", title: "Ferryman of Quad Reef", bio: "Rows the ferry between the reef's islands. Has never once tilted a block.", style: 0, color: 0xFFD8_C07A },
    Member { id: "mei", name: "Mei", title: "Fortune teller of the Bazaar", bio: "Reads your future in the next queue. It is always an S-piece.", style: 3, color: 0xFFC0_3050 },
    Member { id: "toma", name: "Toma", title: "Archivist of Cinder Crown", bio: "Keeps the records of every Commons Cup. Won't talk about the last one.", style: 2, color: 0xFFC8_D0E0 },
    Member { id: "luna", name: "Luna", title: "Moon courier", bio: "Mira's rival in deliveries. Faster, rounder, and much louder about it.", style: 1, color: 0xFF8A_5CF0 },
    Member { id: "kiko", name: "Kiko", title: "Beekeeper of Bloom Valley", bio: "Hana's best friend. Talks to bees, which is still more normal than talking to blobs.", style: 1, color: 0xFF7F_E0B8 },
    Member { id: "jun", name: "Jun", title: "Arcade champion", bio: "Top of every leaderboard until Sora showed up. Hasn't slept since.", style: 0, color: 0xFF9A_A4B8 },
    Member { id: "saki", name: "Saki", title: "Kaede's apprentice", bio: "Practises chains with the Queen every night. Secretly better at them.", style: 1, color: 0xFFB4_A8F0 },
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
    pub right: Vec<&'static str>,
    pub lines: Vec<Line>,
}

/// Speakers with a voice and no body: the puzzle box, and Sena speaking through it.
/// Returns the name tag and its color.
pub fn voice(id: &str) -> Option<(&'static str, u32)> {
    match id {
        "box" => Some(("The box", 0xFFB8_C2DC)),
        "sena" => Some(("Sena", 0xFFFF_D98A)),
        _ => None,
    }
}

pub const EXPRESSIONS: [&str; 8] = ["normal", "smile", "delighted", "angry", "sad", "shocked", "smug", "sleepy"];

/// The story's scenes, parsed once from story.txt (see story.rs).
pub fn scene(id: &str) -> Option<&'static Scene> {
    crate::story::scenes().iter().find(|s| s.id == id)
}
