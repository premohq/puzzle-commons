//! Puzzle Commons client. A custom engine: our own sim, software renderer, font, synth and mixer.
//! The OS layer is only a window that presents our framebuffer, plus native audio and gamepads.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod assets;
mod audio;
mod cast;
mod clock;
mod content;
mod display;
mod font;
mod game;
mod gfx;
mod input;
mod ranks;
mod ranks_ui;
mod render;
mod save;
mod script;
mod settings_ui;
mod theme;
mod tour_ui;
mod ui;

use minifb::{ScaleMode, Window, WindowOptions};
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub fn now_seed() -> u64 {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1);
    let mut x = t ^ 0x9E37_79B9_7F4A_7C15;
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    x & 0xFFFF_FFFF
}

mod png;
mod pool;
mod prof;
mod shots;
mod blocks;
mod story;
mod story_ui;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(|a| a == "--shots").unwrap_or(false) {
        let w = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(1280);
        shots::run(args.get(2).map(|s| s.as_str()).unwrap_or("shots"), w);
        return;
    }
    if args.get(1).map(|a| a == "--story-check").unwrap_or(false) {
        // every scene line checked against the dialogue box; frames saved for the ids given
        let w = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(1280);
        std::process::exit(story::check(args.get(2).map(|s| s.as_str()).unwrap_or("story-shots"), w, &args[args.len().min(4)..]));
    }
    if args.get(1).map(|a| a == "--png-check").unwrap_or(false) {
        // decode a PNG with our own decoder and write it back out (RGB) for comparison
        let data = std::fs::read(&args[2]).expect("read");
        let t0 = Instant::now();
        let img = png::decode(&data).expect("decode failed");
        println!("decoded {}x{} in {:.1} ms", img.w, img.h, t0.elapsed().as_secs_f64() * 1000.0);
        let flat: Vec<u32> = img.px.iter().map(|&p| gfx::mix(0xFF34_406E, p, p >> 24)).collect();
        png::write(&args[3], img.w as usize, img.h as usize, &flat).expect("write");
        return;
    }
    if args.get(1).map(|a| a == "--modes").unwrap_or(false) {
        let m = display::primary();
        println!("primary monitor: {}x{} at ({}, {}), {:.2} Hz", m.w, m.h, m.x, m.y, m.hz);
        for md in display::modes() {
            println!("  {}x{} @ {:.2} Hz", md.w, md.h, md.hz);
        }
        return;
    }
    if args.get(1).map(|a| a == "--banner").unwrap_or(false) {
        shots::banner(args.get(2).map(|s| s.as_str()).unwrap_or("banner.png"));
        return;
    }
    if args.get(1).map(|a| a == "--bench").unwrap_or(false) {
        for w in [1280, 1920, 2560] {
            shots::bench(w);
        }
        return;
    }
    display::init();
    let monitor = display::primary();
    let synth = Arc::new(Mutex::new(audio::Synth::new()));
    let audio_ok = audio::start(synth.clone());
    let input = input::Input::new(None);
    let mut app = app::App::new(input, synth, audio_ok);
    app.monitor = monitor;
    app.modes = display::modes();
    let mut screen = match Screen::open(&app.st, monitor) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Puzzle Commons: could not open a window: {}", e);
            std::process::exit(1);
        }
    };
    app.input.attach(&mut screen.win);
    // --scene ID opens straight on a story scene (writers and testers)
    if args.get(1).map(|a| a == "--scene").unwrap_or(false) {
        if let Some(sc) = args.get(2).and_then(|id| cast::scene(id)) {
            app.screen = app::Screen::Scene { id: sc.id, line: 0, t: 0.0, then: Box::new(app::Screen::Title) };
        }
    }
    let mut present = Presenter::default();
    let mut last = Instant::now();
    let mut next = Instant::now();
    let mut fps_acc = (0u32, Instant::now());
    while screen.win.is_open() {
        // display settings changed: rebuild the window
        if screen.key != Screen::key_of(&app.st) {
            drop(std::mem::replace(&mut screen, match Screen::open(&app.st, monitor) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Puzzle Commons: could not reopen the window: {}", e);
                    break;
                }
            }));
            app.input.attach(&mut screen.win);
            present = Presenter::default();
        }
        let (ww, wh) = screen.win.get_size();
        let (ww, wh) = (ww.max(1), wh.max(1));
        // the canvas: native = the largest 16:9 inside the window, or a fixed render width
        let native = (ww as i32).min(wh as i32 * 16 / 9).max(320);
        let cw = if app.st.render_w == 0 { native } else { app.st.render_w as i32 };
        if app.cv.w != cw {
            app.cv = gfx::Canvas::screen(cw);
        }
        let now = Instant::now();
        let dt = now.duration_since(last).as_secs_f32().min(0.1);
        last = now;
        if !app.frame(Some(&screen.win), dt) {
            break;
        }
        let (map, direct) = present.build(&app.cv, ww, wh, app.st.smooth);
        app.input.mouse_map = map;
        let ok = if direct { screen.win.update_with_buffer(&app.cv.px, ww, wh) } else { screen.win.update_with_buffer(&present.buf, ww, wh) };
        if ok.is_err() {
            break;
        }
        fps_acc.0 += 1;
        let el = fps_acc.1.elapsed().as_secs_f32();
        if el >= 0.5 {
            app.fps = fps_acc.0 as f32 / el;
            fps_acc = (0, Instant::now());
        }
        // pacing: sleep most of the wait, spin the last moment for an even cadence
        let hz = match app.st.fps_cap {
            0 => 0.0,
            1 => {
                if monitor.hz > 20.0 {
                    monitor.hz
                } else {
                    60.0
                }
            }
            // the exact rate when it is the monitor's own (143.88, not 144)
            n if (n as f32 - monitor.hz).abs() < 0.6 => monitor.hz,
            n => n as f32,
        };
        if hz > 0.0 {
            let frame = std::time::Duration::from_secs_f64(1.0 / hz as f64);
            next += frame;
            let now = Instant::now();
            if next > now {
                let wait = next - now;
                if wait > std::time::Duration::from_micros(1500) {
                    std::thread::sleep(wait - std::time::Duration::from_micros(1000));
                }
                while Instant::now() < next {
                    std::hint::spin_loop();
                }
            } else if now - next > frame * 4 {
                next = now;
            }
        } else {
            next = Instant::now();
        }
    }
    app.st.save();
}

/// The OS window and the display settings it was opened with.
struct Screen {
    win: Window,
    key: (save::WindowMode, (u32, u32)),
}
impl Screen {
    fn key_of(st: &save::Settings) -> (save::WindowMode, (u32, u32)) {
        (st.window_mode, if st.window_mode == save::WindowMode::Windowed { st.window_size } else { (0, 0) })
    }
    fn open(st: &save::Settings, mon: display::Monitor) -> Result<Screen, minifb::Error> {
        let key = Screen::key_of(st);
        // tests: PC_WINDOWED keeps the game in a window whatever the settings say
        let mode = if std::env::var_os("PC_WINDOWED").is_some() { save::WindowMode::Windowed } else { st.window_mode };
        let win = match mode {
            save::WindowMode::Fullscreen => {
                let mut w = Window::new(
                    "Puzzle Commons",
                    mon.w.max(320) as usize,
                    mon.h.max(240) as usize,
                    WindowOptions { borderless: true, title: false, resize: true, scale_mode: ScaleMode::UpperLeft, ..WindowOptions::default() },
                )?;
                w.set_position(mon.x as isize, mon.y as isize);
                display::make_fullscreen(&w);
                w
            }
            save::WindowMode::Windowed => {
                // the chosen size, shrunk to fit the monitor with room for decorations
                let (mut ww, mut wh) = st.window_size;
                while (ww > mon.w as u32 || wh + 60 > mon.h as u32) && ww > 1280 {
                    let i = save::WINDOW_SIZES.iter().position(|&s| s == (ww, wh)).unwrap_or(1).max(1) - 1;
                    (ww, wh) = save::WINDOW_SIZES[i];
                }
                let mut w = Window::new("Puzzle Commons", ww as usize, wh as usize, WindowOptions { resize: true, scale_mode: ScaleMode::UpperLeft, ..WindowOptions::default() })?;
                w.set_position((mon.x + (mon.w - ww as i32).max(0) / 2) as isize, (mon.y + (mon.h - wh as i32).max(0) / 2) as isize);
                w
            }
        };
        let mut win = win;
        win.set_target_fps(0);
        Ok(Screen { win, key })
    }
}

/// Puts the canvas in the window: directly when sizes match, otherwise scaled to fit (smooth
/// bilinear or sharp nearest) and letterboxed. Big jobs run on the raster pool.
#[derive(Default)]
struct Presenter {
    buf: Vec<u32>,
    key: (usize, usize, i32, bool),
    geom: (usize, usize, usize, usize),
    xmap: Vec<(u32, u32, u32)>,
}
impl Presenter {
    /// Returns the mouse map (offset x, offset y, pixels per layout unit) and whether the
    /// canvas can be shown as is.
    fn build(&mut self, cv: &gfx::Canvas, ww: usize, wh: usize, smooth: bool) -> ((f32, f32, f32), bool) {
        let (sw, sh) = (cv.w as usize, cv.h as usize);
        // top byte must be zero for macOS' presenter; X11 and Windows ignore it
        let direct = sw == ww && sh == wh && !cfg!(target_os = "macos");
        if direct {
            return ((0.0, 0.0, sw as f32 / gfx::LW as f32), true);
        }
        if self.key != (ww, wh, cv.w, smooth) {
            self.key = (ww, wh, cv.w, smooth);
            let fit = (ww as f32 / sw as f32).min(wh as f32 / sh as f32);
            let dw = ((sw as f32 * fit).round() as usize).clamp(1, ww);
            let dh = ((sh as f32 * fit).round() as usize).clamp(1, wh);
            self.geom = ((ww - dw) / 2, (wh - dh) / 2, dw, dh);
            self.buf = vec![0; ww * wh];
            self.xmap = (0..dw)
                .map(|x| {
                    let g = ((x as f32 + 0.5) * sw as f32 / dw as f32 - 0.5).max(0.0);
                    let x0 = (g as usize).min(sw - 1);
                    (x0 as u32, (x0 + 1).min(sw - 1) as u32, ((g - x0 as f32) * 256.0) as u32)
                })
                .collect();
        }
        let (ox, oy, dw, dh) = self.geom;
        let xmap = &self.xmap;
        let src = &cv.px;
        gfx::par_rows(&mut self.buf, ww, oy as i32, (oy + dh) as i32, dw * dh, |y, row| {
            let y = y as usize - oy;
            let g = ((y as f32 + 0.5) * sh as f32 / dh as f32 - 0.5).max(0.0);
            let y0 = (g as usize).min(sh - 1);
            let y1 = (y0 + 1).min(sh - 1);
            let ty = ((g - y0 as f32) * 256.0) as u32;
            let (r0, r1) = (&src[y0 * sw..y0 * sw + sw], &src[y1 * sw..y1 * sw + sw]);
            let out = &mut row[ox..ox + dw];
            if smooth {
                for (o, &(x0, x1, tx)) in out.iter_mut().zip(xmap.iter()) {
                    let top = gfx::lerp_px(r0[x0 as usize], r0[x1 as usize], tx);
                    let bot = gfx::lerp_px(r1[x0 as usize], r1[x1 as usize], tx);
                    *o = gfx::lerp_px(top, bot, ty);
                }
            } else {
                let r = if ty >= 128 { r1 } else { r0 };
                for (o, &(x0, x1, tx)) in out.iter_mut().zip(xmap.iter()) {
                    *o = r[if tx >= 128 { x1 } else { x0 } as usize] & 0x00FF_FFFF;
                }
            }
        });
        ((ox as f32, oy as f32, dw as f32 / gfx::LW as f32), false)
    }
}
