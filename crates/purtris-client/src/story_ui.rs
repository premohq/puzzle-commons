//! Story scenes (visual-novel style, between story nodes) and the character select screen.

use crate::app::{App, Screen};
use crate::audio::Sfx;
use crate::cast::{self, CAST};
use crate::font::Weight;
use crate::game::Origin;
use crate::gfx::Canvas;
use crate::input::{Device, Nav};
use crate::theme::look;
use crate::ui::{self, GLASS, GLASS_A};
use purtris_sim::{MatchHeader, SeatKind};

/// Character select state: one cursor per seat; bots get characters automatically.
pub struct Select {
    pub header: MatchHeader,
    pub devices: [Option<Device>; 6],
    pub origin: Origin,
    pub cursor: [usize; 6],
    pub ready: [bool; 6],
    /// seconds since everyone was ready (the match starts shortly after)
    pub go_t: f32,
    pub back: Box<Screen>,
}

const SEAT_COLORS: [u32; 6] = [0xFFFF_6B8B, 0xFF4F_B4FF, 0xFF6B_E08A, 0xFFFF_C14D, 0xFFB7_8CFF, 0xFF4F_E0D0];

/// Bots keep their character unless a human took it; then they take the next free one.
fn resolve(header: &MatchHeader, cursor: &[usize; 6], humans: &[usize]) -> [usize; 6] {
    let n = CAST.len();
    let mut chars = *cursor;
    let taken: Vec<usize> = humans.iter().map(|&i| cursor[i]).collect();
    for i in 0..6 {
        if header.seats[i].kind == SeatKind::Bot && taken.contains(&chars[i]) {
            chars[i] = (0..n).map(|d| (chars[i] + d) % n).find(|c| !taken.contains(c) && !chars.iter().enumerate().any(|(j, x)| j != i && x == c)).unwrap_or(chars[i]);
        }
    }
    chars
}

impl App {
    pub(crate) fn scene_seen(&self, id: &str) -> bool {
        self.prog.records.contains_key(&format!("scene.{}", id))
    }
    pub(crate) fn mark_scene(&mut self, id: &str) {
        self.prog.records.insert(format!("scene.{}", id), 1);
        self.prog.save();
    }

    /// Open the select screen for a match header (only human seats choose).
    pub(crate) fn select_for(&mut self, header: MatchHeader, devices: [Option<Device>; 6], origin: Origin, back: Screen) -> Screen {
        let defaults = self.default_chars_pub(&header, &origin);
        let mut ready = [true; 6];
        for i in 0..6 {
            if header.seats[i].kind == SeatKind::Human {
                ready[i] = false;
            }
        }
        Screen::Select(Box::new(Select { header, devices, origin, cursor: defaults, ready, go_t: 0.0, back: Box::new(back) }))
    }

    pub(crate) fn select_screen(&mut self, nav: &Nav, mut s: Box<Select>) -> Screen {
        let l = look(self.st.theme);
        self.bg(0.0);
        ui::title(&mut self.cv, "Choose your characters", "Each player moves with their own keys or gamepad. Confirm to lock in.", &l);
        let n = CAST.len();
        // two rows whatever the cast size: wider rows of smaller tiles for a big cast
        let cols = n.div_ceil(2).max(6);
        let tile = if cols > 6 { (1180 / cols as i32 - 12).min(104) } else { 104 };
        let gap = if cols > 6 { 12 } else { 14 };
        let grid_w = cols as i32 * tile + (cols as i32 - 1) * gap;
        let gx = 640 - grid_w / 2;
        let gy = 116;
        let pos = |i: usize| {
            let (r, c) = (i / cols, i % cols);
            let row_n = if r == n / cols { n % cols } else { cols };
            let row_w = row_n as i32 * tile + (row_n as i32 - 1).max(0) * gap;
            (640 - row_w / 2 + c as i32 * (tile + gap), gy + r as i32 * (tile + gap))
        };
        let _ = gx;
        // input per human seat
        let humans: Vec<usize> = (0..6).filter(|&i| s.header.seats[i].kind == SeatKind::Human).collect();
        let mut back_out = false;
        for &i in &humans {
            let dev = s.devices[i].unwrap_or(Device::KbFull);
            let (lf, rt, up, dn, ok, bk) = self.input.dev_menu(dev);
            if s.ready[i] {
                if bk {
                    s.ready[i] = false;
                    s.go_t = 0.0;
                    self.sfx(Sfx::Back);
                }
                continue;
            }
            let c = s.cursor[i];
            let moved = if lf {
                Some((c + n - 1) % n)
            } else if rt {
                Some((c + 1) % n)
            } else if up {
                Some(if c >= cols { c - cols } else { c })
            } else if dn {
                Some((c + cols).min(n - 1))
            } else {
                None
            };
            if let Some(m) = moved {
                s.cursor[i] = m;
                self.sfx(Sfx::Menu);
            }
            if ok {
                s.ready[i] = true;
                self.sfx(Sfx::Select);
            }
            if bk && i == humans[0] {
                back_out = true;
            }
        }
        // mouse picks for the first human
        if let (Some((mx, my)), Some(&p1)) = (nav.mouse, humans.first()) {
            for k in 0..n {
                let (x, y) = pos(k);
                if mx >= x && mx < x + tile && my >= y && my < y + tile && nav.click && !s.ready[p1] {
                    if s.cursor[p1] == k {
                        s.ready[p1] = true;
                        self.sfx(Sfx::Select);
                    } else {
                        s.cursor[p1] = k;
                        self.sfx(Sfx::Menu);
                    }
                }
            }
        }
        // the grid
        let t = self.t;
        for k in 0..n {
            let (x, y) = pos(k);
            let m = &CAST[k];
            let hovered: Vec<usize> = humans.iter().copied().filter(|&i| s.cursor[i] == k).collect();
            self.cv.glass(x, y, tile, tile, 18, GLASS, if hovered.is_empty() { 150 } else { 110 });
            self.cv.set_clip(x + 3, y + 3, tile - 6, tile - 6);
            crate::assets::draw_portrait(&mut self.cv, m.id, if hovered.is_empty() { "normal" } else { "smile" }, x - 8, y + 2, tile + 16, 255, false);
            self.cv.reset_clip();
            for (j, &i) in hovered.iter().enumerate() {
                let col = SEAT_COLORS[i];
                let inset = j as i32 * 5;
                let pulse = if s.ready[i] { 255 } else { (180.0 + (t * 6.0).sin() * 60.0) as u32 };
                self.cv.round_ring(x - 4 + inset, y - 4 + inset, tile + 8 - 2 * inset, tile + 8 - 2 * inset, 20, 4, col, col, pulse);
                let tag = format!("P{}", i + 1);
                let tw = Canvas::text_wb(&tag, 1, Weight::Black) + 14;
                self.cv.round_rect(x + 4 + j as i32 * (tw + 4), y - 12, tw, 22, 11, col, 255);
                self.cv.text_weight(x + 11 + j as i32 * (tw + 4), y - 9, &tag, 1, 0xFF14_1022, 255, Weight::Black);
            }
        }
        // seat cards: every active seat with its character
        let shown = resolve(&s.header, &s.cursor, &humans);
        let seats: Vec<usize> = (0..6).filter(|&i| s.header.seats[i].kind != SeatKind::Closed).collect();
        let cw = (1180 / seats.len().max(1) as i32).min(300);
        let total = cw * seats.len() as i32;
        let card_y = gy + 2 * (tile + gap) + 10;
        let card_h = 668 - card_y;
        for (j, &i) in seats.iter().enumerate() {
            let x = 640 - total / 2 + j as i32 * cw + 6;
            let w = cw - 12;
            let m = &CAST[shown[i]];
            let human = s.header.seats[i].kind == SeatKind::Human;
            self.cv.glass(x, card_y, w, card_h, 22, GLASS, GLASS_A);
            self.cv.set_clip(x + 2, card_y + 2, w - 4, card_h - 4);
            let expr = if s.ready[i] && human { "delighted" } else if human { "smile" } else { "smug" };
            crate::assets::draw_char_ex(&mut self.cv, m.id, expr, x + w / 2, card_y + card_h + 210, 560, 255, j % 2 == 1);
            self.cv.reset_clip();
            let col = if human { SEAT_COLORS[i] } else { 0xFF9A_A3C0 };
            let who = if human {
                format!("P{}  {}", i + 1, s.devices[i].map(|d| d.name()).unwrap_or_default())
            } else {
                "CPU".to_string()
            };
            self.cv.round_rect(x + 10, card_y + 10, Canvas::text_wb(&who, 1, Weight::Bold) + 20, 24, 12, col, 235);
            self.cv.text_weight(x + 20, card_y + 14, &who, 1, 0xFF14_1022, 255, Weight::Bold);
            let ny = card_y + card_h - 70;
            self.cv.rect_a(x + 2, ny - 8, w - 4, 76, 0xFF06_0A18, 150);
            self.cv.text_weight(x + 16, ny, &cast::name_of(m.id), 3, m.color, 255, Weight::Black);
            self.cv.text_s(x + 16, ny + 36, &cast::say(m.title), 1, l.text);
            if s.ready[i] && human {
                let rw = Canvas::text_wb("Ready!", 2, Weight::Black) + 24;
                self.cv.round_rect(x + w - rw - 10, card_y + 8, rw, 30, 15, l.good, 255);
                self.cv.text_weight(x + w - rw + 2, card_y + 12, "Ready!", 2, 0xFF10_2014, 255, Weight::Black);
            }
        }
        ui::prompts(&mut self.cv, &l, &[("\u{2190} \u{2192} \u{2191} \u{2193}", "Choose"), ("Enter / A", "Lock in"), ("Esc / B", "Back")]);
        if back_out {
            self.sfx(Sfx::Back);
            return *s.back;
        }
        let all = humans.iter().all(|&i| s.ready[i]);
        if all {
            s.go_t += self.dt;
            if s.go_t > 0.6 {
                let chars = resolve(&s.header, &s.cursor, &humans);
                if let Some(&p1) = humans.first() {
                    self.card.character = CAST[chars[p1]].id.to_string();
                    self.card.save();
                }
                self.next_chars = Some(chars);
                let Select { header, devices, origin, .. } = *s;
                return self.start_pub(header, devices, origin, false);
            }
        }
        Screen::Select(s)
    }
}

// ------------------------------------------------------------------ naming the courier

impl App {
    /// Name the story's courier and pick the model (girl or boy). Shown before the prologue
    /// the first time Story opens, and from the player card. An empty field keeps the name.
    pub(crate) fn name_screen(&mut self, nav: &Nav, typed: String, then: Box<Screen>) -> Screen {
        use minifb::Key;
        let l = look(self.st.theme);
        let mut typed = typed;
        for c in self.input.take_typed() {
            if typed.chars().count() < cast::HERO_MAX && (c.is_alphabetic() || matches!(c, ' ' | '-' | '\'')) {
                typed.push(c);
            }
        }
        if self.input.key_pressed(Key::Backspace) {
            typed.pop();
        }
        // the model: arrow keys, the pad's left and right, or a click on either chip
        let (x, y, w) = (560, 80, 640);
        let chip = |k: i32| (x + 40 + k * 190, y + 262, 170, 50);
        let mut boy = self.card.hero_boy;
        let pad = |m: u32| (0..4).any(|i| self.input.pad_pressed(i, m));
        if self.input.key_pressed(Key::Left) || pad(crate::input::pad::LEFT) {
            boy = false;
        }
        if self.input.key_pressed(Key::Right) || pad(crate::input::pad::RIGHT) {
            boy = true;
        }
        if let (Some((mx, my)), true) = (nav.mouse, nav.click) {
            for k in 0..2 {
                let (cx, cy, cw, ch) = chip(k);
                if mx >= cx && mx < cx + cw && my >= cy && my < cy + ch {
                    boy = k == 1;
                }
            }
        }
        if boy != self.card.hero_boy {
            self.card.hero_boy = boy;
            cast::set_hero_boy(boy);
            self.sfx(Sfx::Menu);
        }
        let current = cast::hero();
        let shown = cast::clean_hero(&typed);
        let name = if shown.is_empty() { current.clone() } else { shown.clone() };
        let confirm = self.input.key_pressed(Key::Enter) || pad(crate::input::pad::A);
        let cancel = self.input.key_pressed(Key::Escape) || pad(crate::input::pad::B);
        if confirm || cancel {
            self.card.hero = if confirm { name.clone() } else { current.clone() };
            self.card.save();
            self.sfx(if confirm { Sfx::Select } else { Sfx::Back });
            return *then;
        }

        self.bg_world = 0;
        self.bg_blur(30);
        let bob = if self.st.reduce_motion { 0 } else { ((self.t * 2.0).sin() * 3.0) as i32 };
        crate::assets::draw_char_ex(&mut self.cv, "mira", if shown.is_empty() { "smile" } else { "delighted" }, 300, 1250 + bob, 1130, 255, false);
        self.cv.glass(x, y, w, 470, 30, GLASS, 215);
        self.cv.text_weight(x + 40, y + 34, "Who's carrying the box?", 4, 0xFFFF_FFFF, 255, Weight::Black);
        self.cv.text_s(x + 40, y + 92, &cast::say("Name your courier and choose how {she} looks."), 2, l.dim);
        // the name
        let (fx, fy, fw, fh) = (x + 40, y + 140, w - 80, 86);
        // an outline: the accent shape with the dark field inset inside it
        self.cv.round_rect(fx, fy, fw, fh, 22, l.accent, 255);
        self.cv.round_rect(fx + 3, fy + 3, fw - 6, fh - 6, 19, 0xFF0B_1022, 245);
        let (txt, ink) = if shown.is_empty() { (current.as_str(), 0xFF7C_86A8) } else { (shown.as_str(), 0xFFFF_FFFF) };
        self.cv.text_weight(fx + 28, fy + 18, txt, 5, ink, 255, Weight::Black);
        if self.st.reduce_motion || (self.t * 2.0).fract() < 0.6 {
            let cx = fx + 30 + if shown.is_empty() { 0 } else { Canvas::text_wb(&shown, 5, Weight::Black) };
            self.cv.rect_a(cx, fy + 18, 4, 52, l.accent, 255);
        }
        let count = format!("{} / {}", shown.chars().count(), cast::HERO_MAX);
        self.cv.text_right(fx + fw - 20, fy + fh + 10, &count, 1, l.dim);
        // the model
        for (k, label) in ["Girl", "Boy"].iter().enumerate() {
            let (cx, cy, cw, ch) = chip(k as i32);
            let on = (k == 1) == boy;
            if on {
                self.cv.round_grad(cx, cy, cw, ch, 25, crate::gfx::lighten(l.accent, 40), l.accent, 255);
            } else {
                self.cv.round_rect(cx, cy, cw, ch, 25, 0xFF5A_6488, 255);
                self.cv.round_rect(cx + 2, cy + 2, cw - 4, ch - 4, 23, 0xFF0B_1022, 245);
            }
            let tw = Canvas::text_wb(label, 3, Weight::Black);
            self.cv.text_weight(cx + (cw - tw) / 2, cy + 9, label, 3, if on { 0xFF16_1028 } else { 0xFFC8_D0E8 }, 255, Weight::Black);
        }
        self.cv.text_s(x + 430, y + 278, "\u{2190} \u{2192}", 2, l.dim);
        // how it reads: a line from the first night, with the name in it
        let (kx, ky) = (x + 40, y + 346);
        let kai = cast::get("kai");
        let tag = Canvas::text_wb(kai.name, 2, Weight::Black) + 36;
        self.cv.round_grad(kx, ky, tag, 36, 18, crate::gfx::lighten(kai.color, 40), kai.color, 255);
        self.cv.text_weight(kx + 18, ky + 6, kai.name, 2, 0xFF16_1028, 255, Weight::Black);
        let sample = format!("You're humming it wrong, {}. You always hum it wrong.", name);
        self.cv.text_wrap(kx, ky + 50, w - 80, &sample, 2, 0xFFF4_F6FF);
        ui::prompts(&mut self.cv, &l, &[("Type", "Name"), ("\u{2190} \u{2192}", "Girl / Boy"), ("Enter / A", "Sign"), ("Esc / B", "Keep current")]);
        Screen::NameHero { typed, then }
    }
}
