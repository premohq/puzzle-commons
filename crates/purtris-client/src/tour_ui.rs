//! First-run setup and Mira's guided tour. Mira talks on the left; the right side shows a
//! settings panel (setup) or a diagram drawn from the game's own pieces, maps and cast (tour).

use crate::app::{App, Screen};
use crate::audio::Sfx;
use crate::cast::CAST;
use crate::font::Weight;
use crate::gfx::Canvas;
use crate::input::{Device, Nav};
use crate::settings_ui::Id;
use crate::theme::{self, look, CellKind, SpriteKey};
use crate::ui::{self, ListState, GLASS, GLASS_A};

thread_local! {
    static THUMBS: std::cell::RefCell<std::collections::HashMap<(usize, i32), Canvas>> = std::cell::RefCell::new(std::collections::HashMap::new());
}

struct Page {
    title: &'static str,
    expr: &'static str,
    say: &'static str,
}

const SETUP: [Page; 6] = [
    Page { title: "Welcome!", expr: "delighted", say: "Hi, I'm Mira! Welcome to Puzzle Commons. Let's get your game ready. It only takes a moment, and everything can be changed later in Settings." },
    Page { title: "The picture", expr: "smile", say: "First, the picture. Fullscreen looks best. Match display keeps motion smooth on your monitor. If things feel slow, try a lower quality." },
    Page { title: "Your controls", expr: "smug", say: "The arrow keys are all you need: left and right move, up rotates, down drops faster. Space drops at once. Gamepads work too: just plug one in." },
    Page { title: "Comfort", expr: "smile", say: "Set the volume, and turn on anything that makes the game easier on you. Symbols on every color are on by default." },
    Page { title: "Who are you?", expr: "delighted", say: "Pick who you'll play as in matches. In the story you travel as me, of course!" },
    Page { title: "All set", expr: "delighted", say: "That's everything! Want me to show you around the modes? It's quick, I promise." },
];

const TOUR: [Page; 7] = [
    Page { title: "Four puzzle sports", expr: "delighted", say: "Puzzle Commons has four styles. Quad: fill rows with falling blocks. Chain: pop four of a color and set off chains. Capsule: clear the germs from the bottle. Panel: swap tiles as the stack rises." },
    Page { title: "One match, any mix", expr: "smug", say: "Here's the trick: every player picks their own style, in the same match! When styles mix, attacks travel through a shared gauge and land as the other player's own garbage." },
    Page { title: "Story", expr: "smile", say: "In the story we cross five worlds, one champion each. Win their matches to borrow their style, then face Queen Kaede in Cinder Crown." },
    Page { title: "Versus", expr: "angry", say: "Versus is up to six players on one machine. Two people can share a keyboard, and gamepads are found automatically. Everyone picks a character first." },
    Page { title: "Solo and training", expr: "smile", say: "Solo has sprints, marathons and endless modes with records. In Training I'll teach you each style step by step, with demos and drills." },
    Page { title: "Controls", expr: "smug", say: "Arrows move and rotate. Space hard drops, C holds, Z and X rotate either way. Esc pauses. On a gamepad: d-pad, A and B to rotate, up to drop." },
    Page { title: "Let's play!", expr: "delighted", say: "That's the tour! You can see it again any time from Settings. Now, shall we start with the story?" },
];

impl App {
    fn mira_says(&mut self, title: &str, expr: &str, say: &str, step: (usize, usize)) {
        let l = look(self.st.theme);
        let bob = if self.st.reduce_motion { 0 } else { ((self.t * 1.8).sin() * 3.0) as i32 };
        crate::assets::draw_char(&mut self.cv, "mira", expr, 190, 712 + bob, 640, 255);
        // speech bubble
        self.cv.glass(360, 28, 880, 176, 30, GLASS, 205);
        self.cv.tri([(372.0, 150.0), (344.0, 176.0), (380.0, 170.0)], 0xFF2A_3456, 200);
        self.cv.text_weight(392, 44, &crate::cast::say(title), 4, l.accent, 255, Weight::Black);
        self.cv.text_wrap(392, 98, 820, &crate::cast::say(say), 2, l.text);
        // progress dots
        for k in 0..step.1 {
            let on = k == step.0;
            self.cv.circle(392.0 + k as f32 * 22.0, 188.0, if on { 6.0 } else { 4.0 }, if on { l.accent } else { 0xFF5A_6488 }, 255);
        }
    }

    pub(crate) fn setup_screen(&mut self, nav: &Nav, mut page: usize, mut sel: usize) -> Screen {
        let l = look(self.st.theme);
        self.bg_world = 0;
        self.bg_blur(40);
        let p = &SETUP[page.min(SETUP.len() - 1)];
        self.mira_says(p.title, p.expr, p.say, (page, SETUP.len()));
        let mut forward = nav.ok;
        let mut panel_nav = *nav;
        panel_nav.ok = false;
        match page {
            1 => {
                if let Some(s) = self.settings_panel(&panel_nav, &[Id::WindowMode, Id::RenderRes, Id::FpsCap, Id::Preset], &mut sel, 420, 260, 760) {
                    return s;
                }
            }
            2 => {
                let dev = self.input.last_device.unwrap_or(Device::KbFull);
                self.controls_diagram(400, 236, !matches!(dev, Device::Pad(_)));
            }
            3 => {
                if let Some(s) = self.settings_panel(&panel_nav, &[Id::Music, Id::Effects, Id::Colorblind, Id::ReduceMotion, Id::Shake], &mut sel, 420, 250, 760) {
                    return s;
                }
            }
            4 => {
                // character choice: left / right
                let n = CAST.len();
                let mut ci = crate::cast::index(&self.card.character);
                if nav.left {
                    ci = (ci + n - 1) % n;
                    self.sfx(Sfx::Menu);
                }
                if nav.right {
                    ci = (ci + 1) % n;
                    self.sfx(Sfx::Menu);
                }
                // bosses you haven't beaten can't be picked: step past them the way you were going
                let step = if nav.left { n - 1 } else { 1 };
                for _ in 0..n {
                    if !crate::assets::is_hidden(CAST[ci].id) {
                        break;
                    }
                    ci = (ci + step) % n;
                }
                self.card.character = CAST[ci].id.to_string();
                let m = &CAST[ci];
                self.cv.glass(420, 236, 760, 420, 28, GLASS, GLASS_A);
                self.cv.set_clip(422, 238, 756, 416);
                crate::assets::draw_char(&mut self.cv, m.id, "smile", 590, 980, 820, 255);
                self.cv.reset_clip();
                self.cv.text_weight(760, 280, &crate::cast::name_of(m.id), 5, m.color, 255, Weight::Black);
                self.cv.text_s(762, 344, &crate::cast::say(m.title), 2, l.text);
                self.cv.text_wrap(762, 384, 390, &crate::cast::say(m.bio), 2, l.dim);
                ui::choice(&mut self.cv, &l, 1150, 610, &format!("{} of {}", ci + 1, n), true);
            }
            5 => {
                let opts = ["Take the tour", "Skip to the title screen"];
                if nav.up || nav.down {
                    sel = 1 - sel.min(1);
                    self.sfx(Sfx::Menu);
                }
                for (k, o) in opts.iter().enumerate() {
                    let y = 300 + k as i32 * 86;
                    let on = k == sel.min(1);
                    self.cv.glass(500, y, 600, 66, 33, GLASS, if on { 120 } else { 190 });
                    if on {
                        self.cv.round_rect(500, y, 600, 66, 33, l.accent, 60);
                    }
                    self.cv.text_weight(540, y + 17, o, 3, if on { 0xFFFF_FFFF } else { l.text }, 255, Weight::Bold);
                }
            }
            _ => {
                let msg = "Press Enter, A, or click to begin";
                let w = Canvas::text_wb(msg, 3, Weight::Bold) + 60;
                let a = (((self.t * 2.5).sin() * 0.5 + 0.5) * 155.0 + 100.0) as u32;
                self.cv.glass(800 - w / 2, 420, w, 64, 32, GLASS, 200);
                self.cv.text_weight(800 - w / 2 + 30, 436, msg, 3, l.accent, a, Weight::Bold);
                forward = forward || nav.click;
            }
        }
        ui::prompts(&mut self.cv, &l, &[("\u{2191} \u{2193}", "Choose"), ("\u{2190} \u{2192}", "Change"), ("Enter", "Next"), ("Esc", "Back")]);
        if nav.back && page > 0 {
            self.sfx(Sfx::Back);
            return Screen::Setup { page: page - 1, sel: 0 };
        }
        if forward {
            self.sfx(Sfx::Select);
            if page + 1 < SETUP.len() {
                page += 1;
                return Screen::Setup { page, sel: 0 };
            }
            // finished
            self.st.first_boot = false;
            self.st.save();
            self.card.save();
            return if sel == 0 { Screen::Tour { page: 0, t: 0.0 } } else { Screen::Title };
        }
        Screen::Setup { page, sel }
    }

    pub(crate) fn tour_screen(&mut self, nav: &Nav, mut page: usize, t: f32) -> Screen {
        let l = look(self.st.theme);
        let t = t + self.dt;
        self.bg_world = [0, 1, 2, 3, 4, 0, 1][page.min(6)];
        self.bg_blur(40);
        let p = &TOUR[page.min(TOUR.len() - 1)];
        self.mira_says(p.title, p.expr, p.say, (page, TOUR.len()));
        self.cv.glass(400, 230, 840, 430, 28, GLASS, GLASS_A);
        match page {
            0 => self.diagram_styles(t),
            1 => self.diagram_cross(t),
            2 => self.diagram_story(),
            3 => self.diagram_versus(),
            4 => self.diagram_solo(),
            5 => self.controls_diagram(400, 236, true),
            _ => {
                for (k, id) in ["nami", "hana", "yuki", "rin", "kaede", "kai"].iter().enumerate() {
                    crate::assets::draw_char(&mut self.cv, id, "smile", 470 + k as i32 * 140, 650, 400, 255);
                }
            }
        }
        ui::prompts(&mut self.cv, &l, &[("Enter / \u{2192}", "Next"), ("\u{2190}", "Back"), ("Esc", "Leave the tour")]);
        if nav.back {
            self.sfx(Sfx::Back);
            return Screen::Main(ListState::default());
        }
        if nav.left && page > 0 {
            self.sfx(Sfx::Menu);
            return Screen::Tour { page: page - 1, t: 0.0 };
        }
        if nav.ok || nav.right || nav.click {
            self.sfx(Sfx::Select);
            if page + 1 < TOUR.len() {
                page += 1;
                return Screen::Tour { page, t: 0.0 };
            }
            return Screen::Main(ListState::default());
        }
        Screen::Tour { page, t }
    }

    // ---------------------------------------------------------------- diagrams

    /// A little board built from the real piece sprites. `cells`: (col, row, value).
    #[allow(clippy::too_many_arguments)]
    fn mini_board(&mut self, kind: usize, x: i32, y: i32, cell: i32, cols: i32, rows: i32, cells: &[(i32, i32, u8)], label: &str) {
        let l = look(self.st.theme);
        let accent = [theme::CYAN, theme::GREEN, theme::PINK, theme::YELLOW][kind];
        let (w, h) = (cols * cell, rows * cell);
        self.cv.round_ring(x - 5, y - 5, w + 10, h + 10, 10, 3, accent, crate::gfx::darken(accent, 80), 255);
        self.cv.round_grad(x, y, w, h, 4, 0xFF10_1630, 0xFF07_0A18, 255);
        self.r.sprites.set_scale(self.cv.k);
        for &(c, r, v) in cells {
            let (k, col) = match kind {
                0 => (CellKind::Gem, theme::quad_color(v)),
                1 => (if v == 9 { CellKind::Nuisance } else { CellKind::Blob }, theme::chain_color(if v == 9 { 0 } else { v })),
                2 => (if v >= 10 { CellKind::Virus } else { CellKind::Single }, theme::capsule_color(v % 10)),
                _ => (CellKind::Tile, theme::panel_color(v)),
            };
            let spr = self.r.sprites.get(SpriteKey { kind: k, color: col, size: cell, face: false, pattern: self.st.colorblind, flat: false, frame: 0 });
            self.cv.blit(spr, x + c * cell, y + r * cell, 255);
        }
        let tw = Canvas::text_wb(label, 2, Weight::Bold);
        self.cv.text_weight(x + w / 2 - tw / 2, y + h + 14, label, 2, l.text, 255, Weight::Bold);
    }

    fn diagram_styles(&mut self, t: f32) {
        let quad: Vec<(i32, i32, u8)> = vec![(0, 9, 1), (1, 9, 1), (2, 9, 1), (3, 9, 1), (0, 8, 2), (1, 8, 2), (3, 8, 4), (4, 8, 4), (2, 7, 3), (4, 9, 5), (5, 9, 5), (5, 8, 5), (6, 9, 6), (6, 8, 6), (7, 9, 6)];
        let chain: Vec<(i32, i32, u8)> = vec![(0, 9, 1), (1, 9, 1), (0, 8, 1), (1, 8, 2), (2, 9, 2), (2, 8, 2), (3, 9, 3), (3, 8, 3), (4, 9, 4), (5, 9, 4), (4, 8, 4), (1, 7, 2)];
        let cap: Vec<(i32, i32, u8)> = vec![(1, 9, 12), (3, 8, 11), (5, 9, 13), (2, 6, 1), (2, 7, 2), (4, 5, 3), (4, 6, 3), (0, 9, 2), (6, 7, 11)];
        let pan: Vec<(i32, i32, u8)> = vec![(0, 9, 1), (1, 9, 2), (2, 9, 3), (3, 9, 1), (4, 9, 4), (5, 9, 5), (0, 8, 3), (1, 8, 1), (2, 8, 4), (3, 8, 2), (4, 8, 3), (5, 8, 2), (1, 7, 5), (2, 7, 5), (4, 7, 1)];
        let cell = 26;
        let names = ["Quad", "Chain", "Capsule", "Panel"];
        let cols = [8, 6, 7, 6];
        let mut x = 414;
        for (k, cells) in [quad, chain, cap, pan].iter().enumerate() {
            let lift = if self.st.reduce_motion { 0 } else { ((t * 2.0 + k as f32).sin() * 4.0) as i32 };
            self.mini_board(k, x, 286 + lift, cell, cols[k], 10, cells, names[k]);
            x += cols[k] * cell + 42;
        }
    }

    fn diagram_cross(&mut self, t: f32) {
        let l = look(self.st.theme);
        let chain: Vec<(i32, i32, u8)> = vec![(0, 9, 1), (1, 9, 1), (2, 9, 1), (3, 9, 1), (0, 8, 2), (1, 8, 2), (2, 8, 3), (3, 8, 3), (4, 9, 4), (5, 9, 4)];
        self.mini_board(1, 450, 280, 26, 6, 12, &chain, "Your chain");
        // the attack travels through the gauge
        let ph = (t * 0.6) % 1.0;
        let (x0, x1) = (640.0, 900.0);
        let x = x0 + (x1 - x0) * ph;
        self.cv.line(x0, 420.0, x1, 420.0, 6.0, 0xFF3A_4468, 255);
        self.cv.glow(x, 420.0, 30.0, l.warn, 120);
        self.cv.circle(x, 420.0, 10.0, l.warn, 255);
        self.cv.tri([(x1 + 20.0, 420.0), (x1 - 4.0, 404.0), (x1 - 4.0, 436.0)], 0xFF3A_4468, 255);
        let gw = Canvas::text_wb("cross gauge", 2, Weight::Bold);
        self.cv.round_rect(770 - gw / 2 - 16, 368, gw + 32, 32, 16, 0xFF1C_2340, 230);
        self.cv.text_weight(770 - gw / 2, 373, "cross gauge", 2, l.accent, 255, Weight::Bold);
        let mut quad: Vec<(i32, i32, u8)> = vec![(0, 11, 0), (1, 11, 0), (2, 11, 0), (4, 11, 0), (5, 11, 0), (6, 11, 0), (7, 11, 0), (8, 11, 0), (9, 11, 0), (0, 10, 0), (1, 10, 0), (3, 10, 0), (4, 10, 0), (5, 10, 0), (6, 10, 0), (7, 10, 0), (8, 10, 0), (9, 10, 0)];
        quad.extend([(2, 9, 2), (3, 9, 2), (6, 9, 4), (7, 9, 4)]);
        self.mini_board(0, 950, 280, 22, 10, 12, &quad, "Their garbage rows");
    }

    fn diagram_story(&mut self) {
        let l = look(self.st.theme);
        for w in 0..5usize.min(self.maps.len()) {
            let x = 420 + (w as i32 % 3) * 270;
            let y = 248 + (w as i32 / 3) * 200;
            let (tw, th) = (self.cv.us(240), self.cv.us(135));
            let img = &self.maps[w].image;
            THUMBS.with(|c| {
                let mut c = c.borrow_mut();
                let t = c.entry((w, tw)).or_insert_with(|| crate::assets::resample(img, tw, th));
                self.cv.blit(t, x, y, 255);
            });
            self.cv.round_ring(x - 3, y - 3, 246, 141, 10, 3, 0xFFFF_FFFF, 0xFFAA_B4D0, 200);
            let (rid, rname) = crate::assets::rival(w);
            crate::assets::draw_portrait(&mut self.cv, rid, "smug", x + 168, y + 70, 72, 255, false);
            let name = crate::content::WORLDS[w].name;
            self.cv.text_weight(x + 6, y + 141, &format!("{}  {}", w + 1, name), 1, l.text, 255, Weight::Bold);
            self.cv.text_s(x + 170, y + 141, if crate::assets::is_hidden(rid) { "???" } else { rname }, 1, l.accent);
        }
    }

    fn diagram_versus(&mut self) {
        let l = look(self.st.theme);
        let ids = ["mira", "kai", "luna", "taro", "sora", "emi"];
        let devs = ["Keyboard left", "Keyboard right", "Gamepad 1", "Gamepad 2", "Gamepad 3", "CPU"];
        for k in 0..6 {
            let x = 424 + k as i32 * 136;
            self.cv.glass(x, 250, 124, 390, 20, GLASS, 140);
            self.cv.set_clip(x + 2, 252, 120, 386);
            crate::assets::draw_char(&mut self.cv, ids[k], if k % 2 == 0 { "smile" } else { "smug" }, x + 62, 720, 520, 255);
            self.cv.reset_clip();
            let c = [0xFFFF_6B8B, 0xFF4F_B4FF, 0xFF6B_E08A, 0xFFFF_C14D, 0xFFB7_8CFF, 0xFF9A_A3C0][k];
            let tag = if k < 5 { format!("P{}", k + 1) } else { "CPU".into() };
            self.cv.round_rect(x + 8, 258, Canvas::text_wb(&tag, 1, Weight::Black) + 16, 22, 11, c, 255);
            self.cv.text_weight(x + 16, 261, &tag, 1, 0xFF14_1022, 255, Weight::Black);
            self.cv.rect_a(x + 2, 594, 120, 44, 0xFF06_0A18, 160);
            self.cv.text_s(x + 8, 606, devs[k], 1, l.text);
        }
    }

    fn diagram_solo(&mut self) {
        let l = look(self.st.theme);
        let items = [("timer", "Sprint", "40 lines, fastest time"), ("infinity", "Marathon", "until you top out"), ("puzzle-piece", "Puzzles", "fixed boards, few moves"), ("books", "Lessons", "I teach each style")];
        for (k, (ic, name, sub)) in items.iter().enumerate() {
            let x = 430 + (k as i32 % 2) * 400;
            let y = 260 + (k as i32 / 2) * 190;
            self.cv.glass(x, y, 380, 170, 24, GLASS, 140);
            self.cv.circle((x + 70) as f32, (y + 85) as f32, 46.0, l.accent, 255);
            crate::assets::icon(&mut self.cv, ic, x + 70, y + 85, 54, 0xFF14_1022, 255);
            self.cv.text_weight(x + 136, y + 50, name, 3, 0xFFFF_FFFF, 255, Weight::Black);
            self.cv.text_s(x + 136, y + 96, sub, 2, l.text);
        }
        crate::assets::draw_portrait(&mut self.cv, "mira", "smile", 1110, 540, 110, 255, true);
    }

    /// Keyboard and gamepad diagrams. `kb_first` puts the keyboard on top and highlights it.
    pub(crate) fn controls_diagram(&mut self, x: i32, y: i32, kb_first: bool) {
        let l = look(self.st.theme);
        let key = |cv: &mut Canvas, kx: i32, ky: i32, w: i32, label: &str, hot: bool| {
            cv.round_rect(kx, ky + 4, w, 46, 10, 0xFF05_0812, 200);
            cv.round_grad(kx, ky, w, 46, 10, if hot { crate::gfx::lighten(l.accent, 60) } else { 0xFFE6_EAF6 }, if hot { l.accent } else { 0xFFB8_C0D8 }, 255);
            let tw = Canvas::text_wb(label, 2, Weight::Black);
            cv.text_weight(kx + w / 2 - tw / 2, ky + 9, label, 2, 0xFF16_1A2C, 255, Weight::Black);
        };
        let ky = y + 26;
        self.cv.glass(x + 10, y, 820, 250, 26, GLASS, if kb_first { 150 } else { 200 });
        // arrows
        let ax = x + 560;
        key(&mut self.cv, ax + 62, ky, 56, "\u{2191}", true);
        key(&mut self.cv, ax, ky + 56, 56, "\u{2190}", true);
        key(&mut self.cv, ax + 62, ky + 56, 56, "\u{2193}", true);
        key(&mut self.cv, ax + 124, ky + 56, 56, "\u{2192}", true);
        self.cv.text_s(ax - 6, ky + 120, "move, rotate, drop faster", 1, l.text);
        // space and letters
        key(&mut self.cv, x + 40, ky + 112, 300, "Space", false);
        self.cv.text_s(x + 120, ky + 168, "hard drop (Panel: raise)", 1, l.dim);
        for (k, (lab, what)) in [("Z", "rotate left"), ("X", "rotate right"), ("C", "hold"), ("Esc", "pause")].iter().enumerate() {
            let kx = x + 40 + k as i32 * 128;
            key(&mut self.cv, kx, ky, if *lab == "Esc" { 72 } else { 56 }, lab, false);
            self.cv.text_s(kx, ky + 54, what, 1, l.dim);
        }
        // gamepad
        let gy = y + 262;
        self.cv.glass(x + 10, gy, 820, 150, 26, GLASS, if kb_first { 200 } else { 150 });
        let (cx, cy) = (x + 200, gy + 76);
        self.cv.round_rect(cx - 140, cy - 46, 280, 92, 46, 0xFF2A_3150, 255);
        self.cv.circle((cx - 110) as f32, (cy + 20) as f32, 40.0, 0xFF2A_3150, 255);
        self.cv.circle((cx + 110) as f32, (cy + 20) as f32, 40.0, 0xFF2A_3150, 255);
        self.cv.rect(cx - 104, cy - 8, 48, 16, 0xFFE6_EAF6);
        self.cv.rect(cx - 88, cy - 24, 16, 48, 0xFFE6_EAF6);
        for (k, (bx, by, c)) in [(0, -20, 0xFFF5_D547), (-20, 0, 0xFF47_A8F5), (20, 0, 0xFFE8_475A), (0, 20, 0xFF5B_D07A)].iter().enumerate() {
            let _ = k;
            self.cv.circle((cx + 80 + bx) as f32, (cy + by) as f32, 10.0, *c, 255);
        }
        let lines = ["D-pad: move, up drops", "A / B: rotate (Panel: swap)", "X or LB: hold     Y: rotate 180", "Start: pause"];
        for (k, s) in lines.iter().enumerate() {
            self.cv.text_s(x + 400, gy + 22 + k as i32 * 28, s, 2, l.text);
        }
    }
}
