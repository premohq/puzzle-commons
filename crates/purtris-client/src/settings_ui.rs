//! The settings screen: six tabs over one table of setting definitions. Every row is drawn by
//! the same few widgets (switch, choice, slider, button, info) and changed by its id, never by
//! its position, so adding a setting is one table entry plus its get/change arms.

use crate::app::{App, Screen};
use crate::audio::Sfx;
use crate::font::Weight;
use crate::gfx::Canvas;
use crate::input::Nav;
use crate::save::{WindowMode, FPS_CAPS, RENDER_WIDTHS, WINDOW_SIZES};

/// Resolution choices: native, then every size the monitor offers (fallback: presets).
fn res_choices(modes: &[crate::display::Mode]) -> Vec<(u32, u32)> {
    let mut v = vec![(0u32, 0u32)];
    for m in modes {
        let r = (m.w as u32, m.h as u32);
        if !v.contains(&r) {
            v.push(r);
        }
    }
    if v.len() == 1 {
        v.extend(RENDER_WIDTHS.iter().filter(|&&w| w > 0).map(|&w| (w, w * 9 / 16)));
    }
    v
}

/// Refresh-rate choices: the rates the monitor offers, then unlimited (fallback: presets).
fn hz_choices(modes: &[crate::display::Mode]) -> Vec<u32> {
    let mut v: Vec<u32> = modes.iter().map(|m| m.hz.round() as u32).filter(|&h| h >= 24).collect();
    // plus the usual rates up to the monitor's fastest (some systems only report the current one)
    let top = v.iter().copied().max().unwrap_or(60);
    v.extend([30u32, 60, 75, 90, 100, 120, 144, 165, 240].iter().copied().filter(|&r| r <= top));
    v.sort_unstable_by(|a, b| b.cmp(a));
    v.dedup();
    if v.is_empty() {
        v = FPS_CAPS.iter().copied().filter(|&c| c > 1).collect();
    }
    v.push(0);
    v
}
use crate::theme::{look, Preset, ThemeId};
use crate::ui::{self, cycle, ListState, GLASS, GLASS_A};
use minifb::Key;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Id {
    WindowMode,
    WindowSize,
    RenderRes,
    Scaling,
    FpsCap,
    ShowFps,
    MonitorInfo,
    Theme,
    Preset,
    Characters,
    BesideVersus,
    Faces,
    Bench,
    Music,
    Effects,
    AudioDevice,
    Das,
    Arr,
    SoftDrop,
    Socd,
    Analog,
    InputDisplay,
    Rebind,
    Pads,
    Colorblind,
    ReduceMotion,
    Shake,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Switch,
    Choice,
    Slider,
    Button,
    Info,
}

struct Def {
    tab: usize,
    id: Id,
    kind: Kind,
    label: &'static str,
    desc: &'static str,
}

const TABS: [&str; 6] = ["Display", "Graphics", "Audio", "Gameplay", "Controls", "Accessibility"];

const DEFS: &[Def] = &[
    Def { tab: 0, id: Id::WindowMode, kind: Kind::Choice, label: "Window mode", desc: "Fullscreen covers the whole monitor (borderless, instant to switch). Windowed opens a normal window you can move and resize." },
    Def { tab: 0, id: Id::WindowSize, kind: Kind::Choice, label: "Window size", desc: "The size of the window in windowed mode. Sizes larger than this monitor are skipped." },
    Def { tab: 0, id: Id::RenderRes, kind: Kind::Choice, label: "Resolution", desc: "The resolution the game draws at, from the sizes your monitor supports. Native uses every pixel of the window. A lower one is lighter on slow machines and is scaled up; a higher one gives extra-smooth edges. The game is 16:9, so other shapes get bars." },
    Def { tab: 0, id: Id::Scaling, kind: Kind::Choice, label: "Scaling filter", desc: "How a fixed render resolution is fitted to the window. Smooth blends pixels; Sharp keeps hard pixel edges." },
    Def { tab: 0, id: Id::FpsCap, kind: Kind::Choice, label: "Refresh rate", desc: "How many frames a second the game shows, from the refresh rates your monitor supports. Pick your monitor's rate for the smoothest motion. The match itself always runs at 60 ticks a second." },
    Def { tab: 0, id: Id::ShowFps, kind: Kind::Switch, label: "Show frame rate", desc: "A small counter in the bottom-left corner with frames per second and the render size." },
    Def { tab: 0, id: Id::MonitorInfo, kind: Kind::Info, label: "Monitor", desc: "The primary monitor as the system reports it." },
    Def { tab: 1, id: Id::Theme, kind: Kind::Choice, label: "Theme", desc: "Voyage is the full look with the world maps. Tournament is flat and plain for streams and tournaments. Phosphor is a green CRT look." },
    Def { tab: 1, id: Id::Preset, kind: Kind::Choice, label: "Quality", desc: "Voyage is everything on. Balanced trims effects. Potato is for very slow machines." },
    Def { tab: 1, id: Id::Characters, kind: Kind::Switch, label: "Show characters", desc: "The cast on menus, the story map and results." },
    Def { tab: 1, id: Id::BesideVersus, kind: Kind::Switch, label: "Characters beside boards in versus", desc: "In the story the characters always stand beside the boards and react to the match. In versus and solo it's your choice." },
    Def { tab: 1, id: Id::Faces, kind: Kind::Switch, label: "Faces on pieces", desc: "Little faces on the pieces in two-player matches." },
    Def { tab: 1, id: Id::Bench, kind: Kind::Button, label: "Measure this machine again", desc: "Times a few frames and picks Voyage or Potato quality for you." },
    Def { tab: 2, id: Id::Music, kind: Kind::Slider, label: "Music", desc: "Music volume." },
    Def { tab: 2, id: Id::Effects, kind: Kind::Slider, label: "Sound effects", desc: "Effects volume. You hear a sample as you change it." },
    Def { tab: 2, id: Id::AudioDevice, kind: Kind::Info, label: "Audio output", desc: "Whether a sound device was found when the game started." },
    Def { tab: 3, id: Id::Das, kind: Kind::Slider, label: "Auto-shift delay (DAS)", desc: "Quad: how long a direction is held before the piece starts sliding. Never changes the rules; Chain, Capsule and Panel use their original repeat." },
    Def { tab: 3, id: Id::Arr, kind: Kind::Slider, label: "Auto-repeat rate (ARR)", desc: "Quad: frames between steps while sliding. 0 slides straight to the wall." },
    Def { tab: 3, id: Id::SoftDrop, kind: Kind::Slider, label: "Soft drop speed", desc: "Quad: frames per row while holding down. 0 drops to the floor instantly without locking." },
    Def { tab: 3, id: Id::Socd, kind: Kind::Switch, label: "Opposite directions cancel", desc: "Holding left and right together counts as neither (SOCD neutral), the fair choice for keyboards and hitboxes." },
    Def { tab: 4, id: Id::Rebind, kind: Kind::Button, label: "Rebind keyboard keys", desc: "Change any key for the one-player, left-hand and right-hand keyboard layouts." },
    Def { tab: 4, id: Id::Analog, kind: Kind::Switch, label: "Analog stick moves pieces", desc: "Use the left stick as well as the d-pad during play." },
    Def { tab: 4, id: Id::InputDisplay, kind: Kind::Switch, label: "Input display", desc: "Show the buttons you are pressing under your board." },
    Def { tab: 4, id: Id::Pads, kind: Kind::Info, label: "Gamepads", desc: "Gamepads are found automatically. Plug one in at any time." },
    Def { tab: 5, id: Id::Colorblind, kind: Kind::Switch, label: "Color-blind symbols", desc: "Every piece color also carries a symbol, so no rule depends on color alone." },
    Def { tab: 5, id: Id::ReduceMotion, kind: Kind::Switch, label: "Reduce motion", desc: "Calms panning backgrounds, bobbing and other movement that is not gameplay." },
    Def { tab: 5, id: Id::Shake, kind: Kind::Switch, label: "Screen shake and flashes", desc: "Shake on big attacks and flashes on clears." },
];

/// Display settings that a keep/revert prompt protects.
pub type DisplaySnap = (WindowMode, (u32, u32), u32);


impl App {
    fn snap(&self) -> DisplaySnap {
        (self.st.window_mode, self.st.window_size, self.st.render_w)
    }

    fn value(&self, id: Id) -> (String, bool, i32, i32) {
        let st = &self.st;
        let s = |v: &str| (v.to_string(), false, 0, 0);
        match id {
            Id::WindowMode => s(if st.window_mode == WindowMode::Fullscreen { "Fullscreen" } else { "Windowed" }),
            Id::WindowSize => s(&format!("{} x {}", st.window_size.0, st.window_size.1)),
            Id::RenderRes => {
                if st.render_w == 0 {
                    s("Native")
                } else {
                    let h = res_choices(&self.modes).iter().find(|r| r.0 == st.render_w).map(|r| r.1).unwrap_or(st.render_w * 9 / 16);
                    s(&format!("{} x {}", st.render_w, h))
                }
            }
            Id::Scaling => s(if st.smooth { "Smooth" } else { "Sharp" }),
            Id::FpsCap => match st.fps_cap {
                1 => s(&if self.monitor.hz > 20.0 { format!("{:.0} Hz (this monitor)", self.monitor.hz) } else { "Match display".into() }),
                0 => s("Unlimited"),
                n => s(&format!("{} Hz", n)),
            },
            Id::ShowFps => (String::new(), st.show_fps, 0, 0),
            Id::MonitorInfo => s(&format!("{} x {}{}", self.monitor.w, self.monitor.h, if self.monitor.hz > 0.0 { format!("  at {:.0} Hz", self.monitor.hz) } else { String::new() })),
            Id::Theme => s(st.theme.name()),
            Id::Preset => s(st.preset.name()),
            Id::Characters => (String::new(), st.characters, 0, 0),
            Id::BesideVersus => (String::new(), st.chars_versus, 0, 0),
            Id::Faces => (String::new(), st.decorations, 0, 0),
            Id::Bench => s(""),
            Id::Music => (format!("{}", st.music_vol * 10), false, st.music_vol as i32, 10),
            Id::Effects => (format!("{}", st.sfx_vol * 10), false, st.sfx_vol as i32, 10),
            Id::AudioDevice => s(if self.audio_ok { "Running" } else { "None found (silent)" }),
            Id::Das => (format!("{} ms", st.handling.das * 1000 / 60), false, st.handling.das as i32, 30),
            Id::Arr => (if st.handling.arr == 0 { "Instant".into() } else { format!("{} ms", st.handling.arr * 1000 / 60) }, false, st.handling.arr as i32, 10),
            Id::SoftDrop => (if st.handling.sdf == 0 { "Instant".into() } else { format!("{} ms/row", st.handling.sdf * 1000 / 60) }, false, 10 - st.handling.sdf as i32, 10),
            Id::Socd => (String::new(), st.socd_neutral, 0, 0),
            Id::Rebind => s(""),
            Id::Analog => (String::new(), st.analog, 0, 0),
            Id::InputDisplay => (String::new(), st.input_display, 0, 0),
            Id::Pads => {
                let n = self.input.pad_names();
                s(&if n.is_empty() { "None connected".into() } else { n.join(", ") })
            }
            Id::Colorblind => (String::new(), st.colorblind, 0, 0),
            Id::ReduceMotion => (String::new(), st.reduce_motion, 0, 0),
            Id::Shake => (String::new(), st.shake, 0, 0),
        }
    }

    /// Change a setting by `d` (+1 / -1). Returns a screen to switch to, if any.
    fn change(&mut self, id: Id, d: i32) -> Option<Screen> {
        let before = self.snap();
        let st = &mut self.st;
        let step = |v: u32, d: i32, lo: i32, hi: i32| (v as i32 + d).clamp(lo, hi) as u32;
        match id {
            Id::WindowMode => st.window_mode = if st.window_mode == WindowMode::Fullscreen { WindowMode::Windowed } else { WindowMode::Fullscreen },
            Id::WindowSize => {
                let fits: Vec<(u32, u32)> = WINDOW_SIZES.iter().copied().filter(|&(w, h)| w as i32 <= self.monitor.w && h as i32 + 60 <= self.monitor.h || w == 1280).collect();
                st.window_size = cycle(&fits, if fits.contains(&st.window_size) { st.window_size } else { fits[0] }, d);
            }
            Id::RenderRes => {
                let widths: Vec<u32> = res_choices(&self.modes).iter().map(|r| r.0).collect();
                st.render_w = cycle(&widths, if widths.contains(&st.render_w) { st.render_w } else { 0 }, d);
            }
            Id::Scaling => st.smooth = !st.smooth,
            Id::FpsCap => {
                // "match display" shows as the monitor's own rate in the list
                let mut opts = hz_choices(&self.modes);
                let own = self.monitor.hz.round() as u32;
                let cur = if st.fps_cap == 1 { own } else { st.fps_cap };
                if !opts.contains(&cur) {
                    opts.insert(0, cur);
                }
                let next = cycle(&opts, cur, d);
                st.fps_cap = if next == own && own > 1 { 1 } else { next };
            }
            Id::ShowFps => st.show_fps = !st.show_fps,
            Id::Theme => st.theme = cycle(&ThemeId::ALL, st.theme, d),
            Id::Preset => st.preset = cycle(&Preset::ALL, st.preset, d),
            Id::Characters => st.characters = !st.characters,
            Id::BesideVersus => st.chars_versus = !st.chars_versus,
            Id::Faces => st.decorations = !st.decorations,
            Id::Music => st.music_vol = step(st.music_vol, d, 0, 10),
            Id::Effects => st.sfx_vol = step(st.sfx_vol, d, 0, 10),
            Id::Das => st.handling.das = step(st.handling.das, d, 1, 30),
            Id::Arr => st.handling.arr = step(st.handling.arr, d, 0, 10),
            Id::SoftDrop => st.handling.sdf = step(st.handling.sdf, -d, 0, 10),
            Id::Socd => st.socd_neutral = !st.socd_neutral,
            Id::Analog => st.analog = !st.analog,
            Id::InputDisplay => st.input_display = !st.input_display,
            Id::Colorblind => st.colorblind = !st.colorblind,
            Id::ReduceMotion => st.reduce_motion = !st.reduce_motion,
            Id::Shake => st.shake = !st.shake,
            Id::Bench => {
                self.st.benched = false;
                return Some(Screen::Boot { frames: 0, start: std::time::Instant::now() });
            }
            Id::Rebind => return Some(Screen::Controls(ListState::default(), 0, None)),
            Id::MonitorInfo | Id::AudioDevice | Id::Pads => return None,
        }
        self.apply_settings();
        self.st.save();
        if id == Id::Effects {
            self.sfx(Sfx::Clear(2));
        }
        // a new window or resolution must be confirmed, or it reverts by itself
        let after = self.snap();
        let window_changes = before.0 != after.0 || before.2 != after.2 || (after.0 == WindowMode::Windowed && before.1 != after.1);
        if window_changes {
            let keep_from = self.display_undo.map(|(s, _)| s).unwrap_or(before);
            self.display_undo = Some((keep_from, self.t + 12.0));
        }
        None
    }

    pub fn settings(&mut self, nav: &Nav, mut s: ListState, mut tab: usize) -> Screen {
        self.bg(0.0);
        let l = look(self.st.theme);
        ui::title(&mut self.cv, "Settings", "Saved on this machine. Nothing here changes a match or a replay.", &l);

        // keep / revert prompt after a display change
        if let Some((prev, deadline)) = self.display_undo {
            let left = deadline - self.t;
            let keep = nav.ok || self.input.key_pressed(Key::Enter);
            let revert = nav.back || left <= 0.0;
            self.draw_settings_page(tab, &s, &l, nav, false);
            self.cv.rect_a(0, 0, 1280, 720, 0xFF00_0000, 120);
            self.cv.glass(390, 250, 500, 210, 28, GLASS, 200);
            self.cv.text_weight(430, 276, "Keep these display settings?", 3, 0xFFFF_FFFF, 255, Weight::Black);
            let msg = format!("Going back to the previous ones in {} s.", left.ceil().max(0.0) as i32);
            self.cv.text_s(430, 330, &msg, 2, l.dim);
            ui::choice(&mut self.cv, &l, 860, 400, "Enter: keep     Esc: undo", true);
            if keep {
                self.display_undo = None;
                self.sfx(Sfx::Select);
            } else if revert {
                self.st.window_mode = prev.0;
                self.st.window_size = prev.1;
                self.st.render_w = prev.2;
                self.st.save();
                self.display_undo = None;
                self.say("Display settings restored");
            }
            return Screen::Settings(s, tab);
        }

        // tabs: Q / E, LB / RB, Tab, a click, or arrows: up from the first row puts the focus on
        // the tab bar, where left / right switch tabs and down (or Enter) goes back to the rows
        let prev_tab = tab;
        let on_tabs = self.settings_tabs;
        if self.input.key_pressed(Key::E) || self.input.key_pressed(Key::Tab) || (0..4).any(|i| self.input.pad_pressed(i, crate::input::pad::RB)) || (on_tabs && nav.right) {
            tab = (tab + 1) % TABS.len();
        }
        if self.input.key_pressed(Key::Q) || (0..4).any(|i| self.input.pad_pressed(i, crate::input::pad::LB)) || (on_tabs && nav.left) {
            tab = (tab + TABS.len() - 1) % TABS.len();
        }
        if let Some(t) = ui::tabs_ex(&mut self.cv, &l, &TABS, tab, 112, nav.mouse, nav.click, if on_tabs { Some(self.t) } else { None }) {
            tab = t;
        }
        if on_tabs && (nav.down || nav.ok) {
            self.settings_tabs = false;
            self.sfx(Sfx::Menu);
        }
        if tab != prev_tab {
            s = ListState::default();
            self.sfx(Sfx::Menu);
        }
        let rows: Vec<&Def> = DEFS.iter().filter(|d| d.tab == tab).collect();
        let selectable = |d: &Def| d.kind != Kind::Info;
        if s.sel >= rows.len() || !selectable(rows[s.sel]) {
            s.sel = rows.iter().position(|d| selectable(d)).unwrap_or(0);
        }
        let mv = |s: &mut ListState, dir: i32| {
            let n = rows.len() as i32;
            let mut i = s.sel as i32;
            for _ in 0..n {
                i = (i + dir).rem_euclid(n);
                if selectable(rows[i as usize]) {
                    s.sel = i as usize;
                    break;
                }
            }
        };
        let mut moved = false;
        let first = rows.iter().position(|d| selectable(d)).unwrap_or(0);
        if !on_tabs && nav.up {
            if s.sel == first {
                self.settings_tabs = true;
            } else {
                mv(&mut s, -1);
            }
            moved = true;
        }
        if !on_tabs && nav.down {
            mv(&mut s, 1);
            moved = true;
        }
        if moved {
            self.sfx(Sfx::Menu);
        }
        // mouse: hover a row to select it; click the control to change it
        let (row_y, row_h) = (196, 58);
        let mut clicked = None;
        if let Some((mx, my)) = nav.mouse {
            if mx >= 80 && mx < 800 && my >= row_y {
                let k = ((my - row_y) / row_h) as usize;
                if k < rows.len() && selectable(rows[k]) && nav.click {
                    s.sel = k;
                    clicked = Some(if mx > 560 && mx < 640 { -1 } else { 1 });
                }
            }
        }
        if clicked.is_some() {
            self.settings_tabs = false;
        }
        let cur = rows[s.sel];
        let mut next = None;
        let d = if on_tabs && clicked.is_none() {
            None
        } else if nav.left {
            Some(-1)
        } else if nav.right {
            Some(1)
        } else if nav.ok {
            Some(1)
        } else {
            clicked
        };
        if let Some(d) = d {
            if cur.kind != Kind::Info && (cur.kind != Kind::Button || nav.ok || clicked.is_some()) {
                self.sfx(Sfx::Menu);
                next = self.change(cur.id, d);
            }
        }
        self.draw_settings_page(tab, &s, &l, nav, true);
        if self.settings_tabs {
            ui::prompts(&mut self.cv, &l, &[("\u{2190} \u{2192}", "Tab"), ("\u{2193}", "Settings"), ("Esc", "Back")]);
        } else {
            ui::prompts(&mut self.cv, &l, &[("\u{2191} \u{2193}", "Choose"), ("\u{2190} \u{2192}", "Change"), ("Enter", "Select"), ("Q / E", "Tabs"), ("Esc", "Back")]);
        }
        if let Some(n) = next {
            self.settings_tabs = false;
            return n;
        }
        if nav.back {
            self.settings_tabs = false;
            self.st.save();
            self.sfx(Sfx::Back);
            return Screen::Main(ListState::default());
        }
        Screen::Settings(s, tab)
    }

    fn draw_settings_page(&mut self, tab: usize, s: &ListState, l: &crate::theme::Look, _nav: &Nav, _live: bool) {
        let rows: Vec<&Def> = DEFS.iter().filter(|d| d.tab == tab).collect();
        let (x, w, row_y, row_h) = (80, 720, 196, 58);
        self.cv.glass(x - 24, row_y - 18, w + 48, rows.len() as i32 * row_h + 36, 26, GLASS, GLASS_A);
        let t = self.t;
        for (i, d) in rows.iter().enumerate() {
            let y = row_y + i as i32 * row_h;
            // no row highlight while the tab bar holds the focus
            let sel = i == s.sel && !self.settings_tabs;
            let cy = y + row_h / 2;
            if sel {
                crate::ui::highlight(&mut self.cv, x - 12, y + 4, w + 24, row_h - 8, t);
            }
            let col = if d.kind == Kind::Info { l.dim } else if sel { 0xFFFF_FFFF } else { l.text };
            self.cv.text_weight(x + 6, cy - 12, d.label, 2, col, 255, if sel { Weight::Bold } else { Weight::Regular });
            let (v, on, n, max) = self.value(d.id);
            let rx = x + w - 4;
            match d.kind {
                Kind::Switch => ui::switch(&mut self.cv, l, rx, cy, on),
                Kind::Choice => ui::choice(&mut self.cv, l, rx, cy, &v, sel),
                Kind::Slider => ui::slider(&mut self.cv, l, rx, cy, n, max, &v),
                Kind::Button => {
                    let bw = 46;
                    self.cv.round_rect(rx - bw, cy - 18, bw, 36, 18, if sel { l.accent } else { 0xFF1C_2340 }, 240);
                    let c = if sel { 0xFF14_1022 } else { l.text };
                    self.cv.tri([((rx - 18) as f32, cy as f32), ((rx - 28) as f32, cy as f32 - 7.0), ((rx - 28) as f32, cy as f32 + 7.0)], c, 255);
                }
                Kind::Info => {
                    let tw = Canvas::text_w(&v, 2);
                    self.cv.text_s(rx - tw, cy - 12, &v, 2, l.dim);
                }
            }
        }
        // description of the selected setting
        if let Some(d) = rows.get(s.sel) {
            self.cv.glass(848, 178, 376, 250, 26, GLASS, GLASS_A);
            self.cv.text_weight(872, 198, d.label, 2, crate::gfx::lighten(crate::ui::section(), 110), 255, Weight::Bold);
            self.cv.text_wrap(872, 236, 330, d.desc, 2, l.text);
        }
        if self.st.characters {
            crate::assets::draw_char_ex(&mut self.cv, "aoi", "p-rival", 1110, 664, 250, 255, true);
        }
    }
}

impl App {
    /// A compact panel of chosen settings (used by first-run setup): same widgets, same rules.
    /// Returns a screen to switch to if a row asked for one.
    pub(crate) fn settings_panel(&mut self, nav: &Nav, ids: &[Id], sel: &mut usize, x: i32, y: i32, w: i32) -> Option<Screen> {
        let l = look(self.st.theme);
        let rows: Vec<&Def> = ids.iter().filter_map(|id| DEFS.iter().find(|d| d.id == *id)).collect();
        if rows.is_empty() {
            return None;
        }
        *sel = (*sel).min(rows.len() - 1);
        if nav.up {
            *sel = (*sel + rows.len() - 1) % rows.len();
            self.sfx(Sfx::Menu);
        }
        if nav.down {
            *sel = (*sel + 1) % rows.len();
            self.sfx(Sfx::Menu);
        }
        let row_h = 58;
        let mut next = None;
        let mut clicked = None;
        if let Some((mx, my)) = nav.mouse {
            if nav.click && mx >= x && mx < x + w && my >= y {
                let k = ((my - y) / row_h) as usize;
                if k < rows.len() {
                    *sel = k;
                    clicked = Some(1);
                }
            }
        }
        let cur = rows[*sel];
        let d = if nav.left { Some(-1) } else if nav.right { Some(1) } else { clicked };
        if let Some(d) = d {
            if cur.kind != Kind::Info && cur.kind != Kind::Button {
                self.sfx(Sfx::Menu);
                next = self.change(cur.id, d);
            }
        }
        self.cv.glass(x - 24, y - 18, w + 48, rows.len() as i32 * row_h + 36, 26, GLASS, GLASS_A);
        for (i, d) in rows.iter().enumerate() {
            let ry = y + i as i32 * row_h;
            let cy = ry + row_h / 2;
            let is_sel = i == *sel;
            if is_sel {
                crate::ui::highlight(&mut self.cv, x - 12, ry + 4, w + 24, row_h - 8, self.t);
            }
            self.cv.text_weight(x + 6, cy - 12, d.label, 2, if is_sel { 0xFFFF_FFFF } else { l.text }, 255, if is_sel { Weight::Bold } else { Weight::Regular });
            let (v, on, n, max) = self.value(d.id);
            let rx = x + w - 4;
            match d.kind {
                Kind::Switch => ui::switch(&mut self.cv, &l, rx, cy, on),
                Kind::Choice => ui::choice(&mut self.cv, &l, rx, cy, &v, is_sel),
                Kind::Slider => ui::slider(&mut self.cv, &l, rx, cy, n, max, &v),
                _ => {
                    let tw = Canvas::text_w(&v, 2);
                    self.cv.text_s(rx - tw, cy - 12, &v, 2, l.dim);
                }
            }
        }
        // the keep/revert prompt is the Settings screen's job; setup simply applies
        self.display_undo = None;
        next
    }
}
