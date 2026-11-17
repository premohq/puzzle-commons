//! The Ranks screen: your level and prestige (page 0) and the leaderboards (pages 1..).

use crate::app::{App, Screen};
use crate::audio::Sfx;
use crate::font::Weight;
use crate::gfx::Canvas;
use crate::input::Nav;
use crate::ranks::{self, BOARDS, MAX_LEVEL, PRESTIGE};
use crate::theme::look;
use crate::ui::{self, ListState, GLASS, GLASS_A};
use minifb::Key;

/// A round level badge: prestige-colored ring, the level inside, one pip per prestige.
pub fn level_badge(cv: &mut Canvas, cx: i32, cy: i32, r: i32, level: u32, prestige: u32) {
    let (_, col) = PRESTIGE[(prestige as usize).min(PRESTIGE.len() - 1)];
    let (fx, fy, fr) = (cx as f32, cy as f32, r as f32);
    cv.circle(fx, fy + 2.0, fr + 2.0, 0xFF00_0000, 90);
    cv.circle(fx, fy, fr, col, 255);
    cv.circle(fx, fy, fr * 0.78, 0xFF10_1630, 255);
    let s = format!("{}", level);
    let sc = if r >= 40 { 4 } else if r >= 24 { 2 } else { 1 };
    let tw = Canvas::text_wb(&s, sc, Weight::Black);
    cv.text_weight(cx - tw / 2, cy - (Canvas::px(sc) * 0.5) as i32, &s, sc, 0xFFFF_FFFF, 255, Weight::Black);
    for k in 0..prestige.min(10) {
        let a = -std::f32::consts::FRAC_PI_2 + (k as f32 - (prestige.min(10) as f32 - 1.0) / 2.0) * 0.32;
        cv.star(fx + a.cos() * fr * 1.08, fy + a.sin() * fr * 1.08, (fr * 0.16).max(4.0), 0xFFFF_F0A0, 255);
    }
}

impl App {
    pub(crate) fn ranks_screen(&mut self, nav: &Nav, mut page: usize, mut confirm: bool) -> Screen {
        let l = look(self.st.theme);
        self.bg(0.0);
        let pages = BOARDS.len() + 1;
        if self.input.key_pressed(Key::E) || nav.right || (0..4).any(|i| self.input.pad_pressed(i, crate::input::pad::RB)) {
            page = (page + 1) % pages;
            confirm = false;
            self.sfx(Sfx::Menu);
        }
        if self.input.key_pressed(Key::Q) || nav.left || (0..4).any(|i| self.input.pad_pressed(i, crate::input::pad::LB)) {
            page = (page + pages - 1) % pages;
            confirm = false;
            self.sfx(Sfx::Menu);
        }
        let title = if page == 0 { "Your level".to_string() } else { BOARDS[page - 1].name.to_string() };
        ui::title(&mut self.cv, &title, "Ranks are kept on this machine. Every match earns XP; level 50 unlocks prestige.", &l);
        // page dots
        for k in 0..pages {
            let on = k == page;
            self.cv.circle(640.0 - (pages as f32 - 1.0) * 11.0 + k as f32 * 22.0, 112.0, if on { 6.0 } else { 4.0 }, if on { l.accent } else { 0xFF5A_6488 }, 255);
        }
        let r = &self.ranks;
        if page == 0 {
            // profile: the badge, the bar, the title and the prestige track
            self.cv.glass(80, 140, 700, 520, 30, GLASS, GLASS_A);
            level_badge(&mut self.cv, 210, 290, 90, r.level, r.prestige);
            let (pname, pcol) = PRESTIGE[r.prestige as usize];
            self.cv.text_weight(330, 200, ranks::title(r.level), 4, 0xFFFF_FFFF, 255, Weight::Black);
            let ptxt = if r.prestige == 0 { "No prestige yet".to_string() } else { format!("Prestige {}  -  {}", r.prestige, pname) };
            self.cv.text_weight(332, 258, &ptxt, 2, pcol, 255, Weight::Bold);
            let need = ranks::xp_to_next(r.level);
            let frac = if r.level >= MAX_LEVEL { 1.0 } else { r.xp as f32 / need as f32 };
            self.cv.round_rect(330, 310, 410, 14, 7, 0xFF2A_3150, 255);
            self.cv.round_rect(330, 310, ((410.0 * frac) as i32).max(14), 14, 7, l.accent, 255);
            let bar = if r.level >= MAX_LEVEL { "Max level: you can prestige!".to_string() } else { format!("{} / {} XP to level {}", r.xp, need, r.level + 1) };
            self.cv.text_s(332, 334, &bar, 1, l.text);
            self.cv.text_s(332, 360, &format!("Lifetime XP: {}     XP bonus from prestige: +{}%", r.lifetime, r.prestige * 5), 1, l.dim);
            // the ten tiers
            for (k, (name, col)) in PRESTIGE.iter().enumerate().skip(1) {
                let x = 110 + ((k - 1) as i32 % 5) * 132;
                let y = 430 + ((k - 1) as i32 / 5) * 100;
                let got = r.prestige as usize >= k;
                self.cv.circle((x + 22) as f32, (y + 22) as f32, 20.0, if got { *col } else { 0xFF2A_3150 }, 255);
                self.cv.star((x + 22) as f32, (y + 22) as f32, 11.0, if got { 0xFFFF_F8D0 } else { 0xFF4A_5272 }, 255);
                self.cv.text_s(x - 4, y + 50, name, 1, if got { *col } else { l.dim });
            }
            // prestige button
            let can = r.can_prestige();
            let label = if !can && r.prestige >= ranks::MAX_PRESTIGE {
                "Every prestige earned. You are the Commons Crown!".to_string()
            } else if !can {
                format!("Prestige unlocks at level {}", MAX_LEVEL)
            } else if confirm {
                "Press Enter again: back to level 1, one prestige higher".to_string()
            } else {
                format!("Prestige to {}  (Enter)", PRESTIGE[r.prestige as usize + 1].0)
            };
            let w = Canvas::text_wb(&label, 2, Weight::Bold) + 48;
            self.cv.glass(840, 560, w.min(400), 56, 28, GLASS, 200);
            if can {
                self.cv.round_rect(840, 560, w.min(400), 56, 28, if confirm { l.warn } else { l.accent }, 70);
            }
            self.cv.set_clip(840, 560, 400, 56);
            self.cv.text_weight(864, 574, &label, 2, if can { 0xFFFF_FFFF } else { l.dim }, 255, Weight::Bold);
            self.cv.reset_clip();
            if self.st.characters {
                crate::assets::draw_char(&mut self.cv, &self.card.character.clone(), if r.level >= 25 { "delighted" } else { "smile" }, 1030, 540, 400, 255);
            }
            if nav.ok && can {
                if confirm {
                    self.ranks.do_prestige();
                    self.sfx(Sfx::Win);
                    self.say(format!("Prestige {}: {}!", self.ranks.prestige, PRESTIGE[self.ranks.prestige as usize].0));
                    confirm = false;
                } else {
                    confirm = true;
                    self.sfx(Sfx::Menu);
                }
            }
        } else {
            let b = &BOARDS[page - 1];
            let empty = Vec::new();
            let list = r.boards.get(b.id).unwrap_or(&empty);
            self.cv.glass(60, 140, 880, 530, 30, GLASS, GLASS_A);
            let cols = [("#", 92), ("Player", 200), ("Level", 520), (if b.lower { "Time" } else { "Best" }, 740), ("Date", 860)];
            for (h, x) in cols {
                self.cv.text_weight(x, 158, h, 1, l.accent, 255, Weight::Bold);
            }
            if list.is_empty() {
                self.cv.text_s(200, 260, "No scores yet. Play this mode to set the first one!", 2, l.dim);
            }
            let me = ui::nice(&self.card.name);
            for (k, e) in list.iter().enumerate().take(10) {
                let y = 186 + k as i32 * 47;
                if e.name == me {
                    self.cv.round_rect(74, y - 4, 852, 42, 21, l.accent, 34);
                }
                let medal = [0xFFF5_C84A, 0xFFC8_D2E2, 0xFFCD_8A4F];
                if k < 3 {
                    self.cv.circle(100.0, y as f32 + 17.0, 15.0, medal[k], 255);
                    self.cv.text_weight(95, y + 6, &format!("{}", k + 1), 2, 0xFF14_1022, 255, Weight::Black);
                } else {
                    self.cv.text_weight(92, y + 6, &format!("{}", k + 1), 2, l.text, 255, Weight::Bold);
                }
                let m = crate::cast::get(&e.character);
                self.cv.circle(160.0, y as f32 + 17.0, 18.0, m.color, 255);
                self.cv.set_clip(140, y - 6, 40, 42);
                crate::assets::draw_portrait(&mut self.cv, m.id, if k == 0 { "delighted" } else { "smile" }, 136, y - 4, 48, 255, false);
                self.cv.reset_clip();
                self.cv.text_weight(200, y + 6, &e.name, 2, 0xFFFF_FFFF, 255, Weight::Bold);
                self.cv.text_s(200 + Canvas::text_wb(&e.name, 2, Weight::Bold) + 12, y + 11, &crate::cast::name_of(m.id), 1, l.dim);
                level_badge(&mut self.cv, 536, y + 17, 15, e.level, e.prestige);
                self.cv.text_s(560, y + 11, ranks::title(e.level), 1, l.dim);
                self.cv.text_weight(740, y + 6, &ranks::fmt_value(b, e.value), 2, if k == 0 { l.accent } else { l.text }, 255, Weight::Bold);
                let days = (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0).saturating_sub(e.when)) / 86_400;
                let when = if days == 0 { "today".to_string() } else if days == 1 { "yesterday".to_string() } else { format!("{} days ago", days) };
                self.cv.text_s(860, y + 11, &when, 1, l.dim);
            }
            // side card: your best and level
            self.cv.glass(980, 140, 250, 300, 26, GLASS, GLASS_A);
            level_badge(&mut self.cv, 1105, 220, 48, r.level, r.prestige);
            self.cv.text_weight(1000, 286, &me, 2, 0xFFFF_FFFF, 255, Weight::Black);
            self.cv.text_s(1000, 320, ranks::title(r.level), 1, l.dim);
            let mine = list.iter().position(|e| e.name == me);
            let txt = match mine {
                Some(p) => format!("Your best: #{}  {}", p + 1, ranks::fmt_value(b, list[p].value)),
                None => "Not on this board yet".into(),
            };
            self.cv.text_wrap(1000, 352, 210, &txt, 1, l.accent);
        }
        ui::prompts(&mut self.cv, &l, &[("\u{2190} \u{2192} / Q E", "Boards"), ("Enter", "Prestige"), ("Esc", "Back")]);
        if nav.back {
            self.sfx(Sfx::Back);
            return Screen::Main(ListState::default());
        }
        Screen::Ranks { page, confirm }
    }
}
