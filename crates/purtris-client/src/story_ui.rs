//! Story scenes (visual-novel style, between story nodes) and the character select screen.

use crate::app::{App, Screen};
use crate::audio::Sfx;
use crate::cast::CAST;
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
            self.cv.text_weight(x + 16, ny, m.name, 3, m.color, 255, Weight::Black);
            self.cv.text_s(x + 16, ny + 36, m.title, 1, l.text);
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
