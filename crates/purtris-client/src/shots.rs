//! Headless screenshots of real engine frames: `dr-purtris --shots DIR`.

use crate::app::{App, Screen};
use purtris_sim::Ev;
use crate::game::{Game, Origin};
use crate::ui::ListState;
use purtris_sim::*;
use std::sync::{Arc, Mutex};

fn save(app: &App, dir: &str, name: &str) {
    let p = format!("{}/{}.png", dir, name);
    crate::png::write(&p, app.cv.w as usize, app.cv.h as usize, &app.cv.px).unwrap();
    println!("wrote {}", p);
}

fn match_shot(app: &mut App, dir: &str, name: &str, h: MatchHeader, ticks: u32) {
    let names: [String; 6] = std::array::from_fn(|i| if h.seats[i].kind == SeatKind::Closed { String::new() } else { format!("CPU{}", i + 1) });
    let mut g = Game::new(h, [None; 6], names, Origin::Replay, true).unwrap();
    g.humans[0] = true;
    g.chars = [0, 2, 3, 4, 5, 7];
    for _ in 0..ticks / 60 {
        g.fast_forward(60, &app.input, &app.st, &app.synth, &mut app.r);
        app.screen = Screen::Play(Box::new(std::mem::replace(&mut g, Game::new(h, [None; 6], Default::default(), Origin::Replay, true).unwrap())));
        app.frame(None, 1.0 / 60.0);
        if let Screen::Play(gg) = std::mem::replace(&mut app.screen, Screen::Title) {
            g = *gg;
        }
    }
    app.screen = Screen::Play(Box::new(g));
    app.frame(None, 1.0 / 60.0);
    save(app, dir, name);
}

fn bot(p: Profile) -> (SeatKind, Profile, Personality, u8) {
    (SeatKind::Bot, p, Personality::Builder, 6u8)
}

/// Time real frames of the heaviest screens at a given pixel width.
pub fn bench(width: i32) {
    let synth = Arc::new(Mutex::new(crate::audio::Synth::new()));
    let input = crate::input::Input::new(None);
    let mut app = App::new(input, synth, false);
    app.st = crate::save::Settings::default();
    app.st.benched = true;
    app.apply_settings();
    app.cv = crate::gfx::Canvas::screen(width);
    let h = quick_header(7, Mode::Versus, &[bot(Profile::QuadPpt), bot(Profile::ChainTsu), bot(Profile::CapsuleVs), bot(Profile::PanelPdp)]);
    let mut g = Game::new(h, [None; 6], Default::default(), Origin::Replay, true).unwrap();
    g.fast_forward(60 * 20, &app.input, &app.st, &app.synth, &mut app.r);
    let screens: Vec<(&str, Box<dyn Fn() -> Screen>)> = vec![
        ("title", Box::new(|| Screen::Title)),
        ("menu", Box::new(|| Screen::Main(ListState::default()))),
        ("story", Box::new(|| Screen::Story { world: 0, node: 0, diff: 1, chosen: 0, talk: true })),
        ("settings", Box::new(|| Screen::Settings(ListState::default(), 0))),
    ];
    println!("frame times at {}x{} (k = {:.2})", app.cv.w, app.cv.h, app.cv.k);
    {
        let cv = &mut app.cv;
        let time = |name: &str, cv: &mut crate::gfx::Canvas, f: &dyn Fn(&mut crate::gfx::Canvas)| {
            let t0 = std::time::Instant::now();
            for _ in 0..20 {
                f(cv);
            }
            println!("    prim {:22} {:6.2} ms", name, t0.elapsed().as_secs_f64() * 1000.0 / 20.0);
        };
        time("glass 860x560", cv, &|cv| cv.glass(90, 120, 860, 560, 22, 0xFF0A1124, 165));
        time("glass 300x40", cv, &|cv| cv.glass(90, 120, 300, 40, 20, 0xFF0A1124, 165));
        time("soft_shadow 860x560", cv, &|cv| cv.soft_shadow(90, 120, 860, 560, 22, 10, 70));
        time("round_rect 860x560", cv, &|cv| cv.round_rect(90, 120, 860, 560, 22, 0xFF0A1124, 165));
        time("rect_a full", cv, &|cv| cv.rect_a(0, 0, 1280, 720, 0xFF0A1124, 80));
        time("25 text lines", cv, &|cv| {
            for i in 0..25 {
                cv.text_s(100, 100 + i * 24, "Colorblind patterns and some value text", 2, 0xFFFFFFFF);
            }
        });
        time("glow r70", cv, &|cv| cv.glow(300.0, 300.0, 70.0, 0xFF88AAFF, 40));
        time("board shadow", cv, &|cv| cv.soft_shadow(300, 100, 276, 536, 12, 7, 130));
        time("board ring", cv, &|cv| cv.round_ring(300, 100, 276, 536, 12, 3, 0xFF88AAFF, 0xFF224466, 255));
        time("board well", cv, &|cv| cv.round_grad(308, 108, 260, 520, 4, 0xFF101630, 0xFF070A18, 255));
        time("board wash", cv, &|cv| cv.rect_a(308, 108 + 416, 260, 104, 0xFF88AAFF, 10));
        time("plate glass", cv, &|cv| cv.glass(300, 40, 276, 50, 25, 0xFF0A1124, 175));
        time("grid 30 lines", cv, &|cv| {
            for i in 0..30 {
                cv.rect_a(308 + i * 9, 108, 1, 520, 0xFF334466, 70);
            }
        });
    }
    for (name, mk) in screens.iter() {
        app.screen = mk();
        app.frame(None, 0.016);
        crate::prof::enable(true);
        let t0 = std::time::Instant::now();
        for _ in 0..30 {
            app.frame(None, 0.016);
        }
        println!("  {:10} {:6.2} ms", name, t0.elapsed().as_secs_f64() * 1000.0 / 30.0);
        crate::prof::report(30);
        crate::prof::enable(false);
    }
    app.screen = Screen::Play(Box::new(g));
    app.frame(None, 0.016);
    crate::prof::enable(true);
    let t0 = std::time::Instant::now();
    for _ in 0..30 {
        app.frame(None, 0.016);
    }
    println!("  {:10} {:6.2} ms", "4p match", t0.elapsed().as_secs_f64() * 1000.0 / 30.0);
    crate::prof::report(30);
    crate::prof::enable(false);
    let h6 = quick_header(9, Mode::Versus, &[bot(Profile::QuadPpt), bot(Profile::ChainTsu), bot(Profile::CapsuleVs), bot(Profile::PanelPdp), bot(Profile::QuadPpt), bot(Profile::PanelPdp)]);
    let mut g6 = Game::new(h6, [None; 6], Default::default(), Origin::Replay, true).unwrap();
    g6.fast_forward(60 * 20, &app.input, &app.st, &app.synth, &mut app.r);
    app.screen = Screen::Play(Box::new(g6));
    app.frame(None, 0.016);
    crate::prof::enable(true);
    let t0 = std::time::Instant::now();
    for _ in 0..30 {
        app.frame(None, 0.016);
    }
    println!("  {:10} {:6.2} ms", "6p match", t0.elapsed().as_secs_f64() * 1000.0 / 30.0);
    crate::prof::report(30);
    crate::prof::enable(false);
    // heavy effects, with screen flashes and shake on, re-staged every half second:
    //   "6p chain"  your board fires a 7-chain (or a B2B T-spin triple) and an attack while
    //               the five bots each clear a 2-chain or a double and attack too;
    //   "6p stress" every board fires the big move at once, plus a KO.
    app.st.shake = true;
    if let Screen::Play(g) = &mut app.screen {
        g.hold = true;
    }
    for (name, big, ko) in [("6p chain", 1usize, false), ("6p stress", 6, true)] {
        app.r.fx = crate::fx::Fx::new();
        app.frame(None, 0.0);
        crate::prof::enable(true);
        let mut worst = 0.0f64;
        let t0 = std::time::Instant::now();
        let frames = 90;
        for f in 0..frames {
            if f % 30 == 0 {
                if let Screen::Play(g) = &app.screen {
                    app.r.fx.stage(&g.m, &app.r.geoms, &app.st, 7, ko, big);
                }
            }
            // PC_BENCH_PEAK: profile only the first quarter second after each big move
            if std::env::var_os("PC_BENCH_PEAK").is_some() {
                crate::prof::pause(f % 30 >= 15);
            }
            let t1 = std::time::Instant::now();
            app.frame(None, 1.0 / 60.0);
            let ft = t1.elapsed().as_secs_f64();
            if std::env::var_os("PC_BENCH_FRAMES").is_some() {
                println!("      frame {:2} {:6.2} ms", f, ft * 1000.0);
            }
            worst = worst.max(ft);
        }
        println!("  {:10} {:6.2} ms   worst {:6.2} ms", name, t0.elapsed().as_secs_f64() * 1000.0 / frames as f64, worst * 1000.0);
        crate::prof::report(if std::env::var_os("PC_BENCH_PEAK").is_some() { frames / 2 } else { frames });
        crate::prof::enable(false);
    }
    crate::prof::enable(false);
}

pub fn run(dir: &str, width: i32) {
    let _ = std::fs::create_dir_all(dir);
    let synth = Arc::new(Mutex::new(crate::audio::Synth::new()));
    let input = crate::input::Input::new(None);
    let mut app = App::new(input, synth, false);
    app.st = crate::save::Settings::default();
    app.st.benched = true;
    app.apply_settings();
    app.cv = crate::gfx::Canvas::screen(width);
    app.screen = Screen::Title;
    app.frame(None, 0.016);
    save(&app, dir, "01_title");
    app.screen = Screen::Main(ListState::default());
    app.frame(None, 0.016);
    save(&app, dir, "02_menu");
    app.screen = Screen::Story { world: 0, node: 0, diff: 1, chosen: 0, talk: true };
    app.frame(None, 0.016);
    save(&app, dir, "03_story");
    app.screen = Screen::Versus(Box::new(crate::app::Room::new()));
    app.frame(None, 0.016);
    save(&app, dir, "04_room");
    let h = quick_header(11, Mode::Versus, &[bot(Profile::QuadPpt), bot(Profile::ChainTsu)]);
    match_shot(&mut app, dir, "05_quad_vs_chain", h, 60 * 40);
    let h = quick_header(12, Mode::Versus, &[bot(Profile::CapsuleVs), bot(Profile::PanelPdp)]);
    match_shot(&mut app, dir, "06_capsule_vs_panel", h, 60 * 30);
    let h = quick_header(13, Mode::Versus, &[bot(Profile::QuadPpt), bot(Profile::ChainTsu), bot(Profile::CapsuleVs), bot(Profile::PanelPdp), bot(Profile::QuadPpt), bot(Profile::PanelPdp)]);
    match_shot(&mut app, dir, "07_six_seats", h, 60 * 25);
    app.st.theme = crate::theme::ThemeId::Tournament;
    app.apply_settings();
    let h = quick_header(14, Mode::Versus, &[bot(Profile::PanelPdp), bot(Profile::PanelPdp)]);
    match_shot(&mut app, dir, "08_tournament_panel", h, 60 * 35);
    app.st.theme = crate::theme::ThemeId::Phosphor;
    app.apply_settings();
    let h = quick_header(15, Mode::Versus, &[bot(Profile::QuadPpt), bot(Profile::QuadPpt)]);
    match_shot(&mut app, dir, "09_phosphor_quad", h, 60 * 30);
    app.st.theme = crate::theme::ThemeId::Voyage;
    app.apply_settings();
    app.screen = Screen::Lesson(1, ListState::default(), 0);
    app.frame(None, 0.016);
    save(&app, dir, "10_lesson");
    app.screen = Screen::Calc(ListState::default(), 5, 4, 70, 0);
    app.frame(None, 0.016);
    save(&app, dir, "11_calc");
    app.screen = Screen::Settings(ListState::default(), 0);
    app.frame(None, 0.016);
    save(&app, dir, "12_settings");
    let screens: Vec<(&str, Screen)> = vec![
        ("13_curriculum", Screen::Curriculum(ListState::default(), 4)),
        ("14_card", Screen::Card(ListState::default(), None)),
        ("15_help", Screen::Help(4)),
        ("16_controls", Screen::Controls(ListState::default(), 1, None)),
        ("17_story_w5", Screen::Story { world: 4, node: 3, diff: 2, chosen: 2, talk: true }),
        ("18_training", Screen::Training(ListState::default())),
        ("19_seeds", Screen::Seeds(ListState::default(), 0xC0FFEE, String::new())),
        ("20_solo", Screen::Solo(ListState::default())),
    ];
    for (n, sc) in screens {
        app.screen = sc;
        app.frame(None, 0.016);
        save(&app, dir, n);
    }
    let h = quick_header(1, Mode::PanelPuzzle(5), &[(SeatKind::Bot, Profile::PanelPdp, Personality::Builder, 1)]);
    match_shot(&mut app, dir, "21_puzzle", h, 30);
    let mut h = quick_header(21, Mode::Versus, &[bot(Profile::ChainTsu), bot(Profile::CapsuleVs), bot(Profile::QuadPpt), bot(Profile::PanelPdp)]);
    for i in 0..4 { h.seats[i].team = 1 + (i as u8 / 2); }
    h.target = TargetRule::Team;
    match_shot(&mut app, dir, "22_two_v_two", h, 60 * 45);
    app.st.preset = crate::theme::Preset::Potato;
    app.apply_settings();
    let h = quick_header(22, Mode::Versus, &[bot(Profile::ChainTsu), bot(Profile::CapsuleVs)]);
    match_shot(&mut app, dir, "23_potato", h, 60 * 30);
    app.st.preset = crate::theme::Preset::Voyage;
    app.apply_settings();
    let h = quick_header(23, Mode::Versus, &[bot(Profile::QuadPpt), (SeatKind::Bot, Profile::ChainTsu, Personality::Builder, 1u8)]);
    let names: [String; 6] = std::array::from_fn(|i| format!("CPU{}", i + 1));
    let mut g = Game::new(h, [None; 6], names, Origin::Solo(Mode::Versus, Profile::QuadPpt), false).unwrap();
    g.fast_forward(60 * 20, &app.input, &app.st, &app.synth, &mut app.r);
    g.paused = true;
    app.screen = Screen::Play(Box::new(g));
    app.frame(None, 0.016);
    save(&app, dir, "24_pause");
    if let Screen::Play(mut g) = std::mem::replace(&mut app.screen, Screen::Title) {
        g.paused = false;
        g.fast_forward(60 * 300, &app.input, &app.st, &app.synth, &mut app.r);
        g.over_t = 500;
        app.screen = Screen::Play(g);
        app.frame(None, 0.016);
        app.frame(None, 0.016);
        save(&app, dir, "25_results");
    }
    fx_shots(dir, width, None);
}

/// Show a held game for one frame (the match clock stands still; effects advance by `dt`).
fn show(app: &mut App, mut g: Game, dt: f32) -> Game {
    g.hold = true;
    app.screen = Screen::Play(Box::new(g));
    app.frame(None, dt);
    match std::mem::replace(&mut app.screen, Screen::Title) {
        Screen::Play(g) => *g,
        _ => panic!("left the match screen"),
    }
}

fn fx_game(app: &mut App, h: MatchHeader) -> Game {
    let names: [String; 6] = std::array::from_fn(|i| if h.seats[i].kind == SeatKind::Closed { String::new() } else { format!("CPU{}", i + 1) });
    let mut g = Game::new(h, [None; 6], names, Origin::Replay, false).unwrap();
    g.humans[0] = true;
    g.chars = [0, 2, 3, 4, 5, 7];
    g.countdown = 0;
    app.r.fx = crate::fx::Fx::new();
    // one frame so the renderer knows where the boards are
    show(app, g, 0.0)
}

/// Play a bot match until `pred` matches an event, then shoot frames `offsets` ticks later.
fn hunt(app: &mut App, dir: &str, name: &str, h: MatchHeader, max_ticks: u32, offsets: &[u32], pred: &dyn Fn(&Match, u8, &Ev) -> bool) -> bool {
    let mut g = fx_game(app, h);
    let mut t = 0;
    while t < max_ticks && !g.finished() {
        g.fast_forward(1, &app.input, &app.st, &app.synth, &mut app.r);
        t += 1;
        if t % 30 == 0 {
            g = show(app, g, 0.0);
        }
        if g.m.events.iter().any(|(s, e)| pred(&g.m, *s, e)) {
            let mut at = 0;
            for &o in offsets {
                // effects only (the final K.O. ends the match, so the sim may have stopped)
                while at < o {
                    if g.finished() {
                        app.r.fx.update(1.0 / 60.0);
                    } else {
                        g.fast_forward(1, &app.input, &app.st, &app.synth, &mut app.r);
                    }
                    at += 1;
                }
                g = show(app, g, 0.0);
                save(app, dir, &format!("{}_{:02}", name, o));
            }
            return true;
        }
    }
    println!("{}: no moment within {} ticks", name, max_ticks);
    false
}

/// Effect moments: real ones found in bot matches, and staged heavy ones.
pub fn fx_shots(dir: &str, width: i32, only: Option<&str>) {
    let _ = std::fs::create_dir_all(dir);
    let synth = Arc::new(Mutex::new(crate::audio::Synth::new()));
    let input = crate::input::Input::new(None);
    let mut app = App::new(input, synth, false);
    app.st = crate::save::Settings::default();
    app.st.benched = true;
    app.st.shake = true;
    app.apply_settings();
    app.cv = crate::gfx::Canvas::screen(width);
    let want = |n: &str| only.map(|o| n.contains(o)).unwrap_or(true);
    let strong = |p: Profile| (SeatKind::Bot, p, Personality::Builder, 9u8);
    if want("chain") {
        let h = quick_header(31, Mode::Versus, &[strong(Profile::ChainTsu), strong(Profile::ChainTsu)]);
        hunt(&mut app, dir, "fx_chain", h, 60 * 240, &[3, 10, 22, 40], &|_, _, e| matches!(e, Ev::ChainStep { n } if *n >= 3));
    }
    if want("quad") {
        let h = quick_header(32, Mode::Versus, &[strong(Profile::QuadPpt), strong(Profile::QuadPpt)]);
        hunt(&mut app, dir, "fx_quad", h, 60 * 240, &[2, 8, 16], &|_, _, e| matches!(e, Ev::Clear { text, .. } if text.contains("QUAD") || text.contains("T-SPIN D") || text.contains("T-SPIN T")));
    }
    if want("tspin") {
        // bots rarely land T-spins, so this one is staged: a B2B T-spin triple with a combo
        let h = quick_header(11, Mode::Versus, &[bot(Profile::QuadPpt), bot(Profile::ChainTsu)]);
        let mut g = fx_game(&mut app, h);
        g.fast_forward(60 * 30, &app.input, &app.st, &app.synth, &mut app.r);
        g = show(&mut app, g, 0.0);
        app.r.fx = crate::fx::Fx::new();
        app.r.fx.k = app.cv.k;
        app.r.fx.stage(&g.m, &app.r.geoms, &app.st, 5, false, 2);
        let mut prev = 0;
        for f in [3u32, 9, 20] {
            for _ in prev..f {
                app.r.fx.update(1.0 / 60.0);
            }
            prev = f;
            g = show(&mut app, g, 0.0);
            save(&app, dir, &format!("fx_tspin_staged_{:02}", f));
        }
    }
    if want("panel") {
        let h = quick_header(33, Mode::Versus, &[strong(Profile::PanelPdp), strong(Profile::PanelPdp)]);
        hunt(&mut app, dir, "fx_panel", h, 60 * 240, &[4, 50, 64], &|_, _, e| matches!(e, Ev::ChainStep { n } if *n >= 2));
    }
    if want("capsule") {
        let h = quick_header(34, Mode::Versus, &[strong(Profile::CapsuleVs), strong(Profile::CapsuleVs)]);
        hunt(&mut app, dir, "fx_capsule", h, 60 * 240, &[3, 10], &|_, _, e| matches!(e, Ev::Clear { text, .. } if !text.is_empty()));
    }
    if want("attack") {
        let h = quick_header(35, Mode::Versus, &[strong(Profile::QuadPpt), strong(Profile::ChainTsu), strong(Profile::CapsuleVs), strong(Profile::PanelPdp)]);
        hunt(&mut app, dir, "fx_attack", h, 60 * 240, &[8, 18, 29], &|_, _, e| matches!(e, Ev::AttackSent { units } if *units >= 3));
    }
    if want("ko") {
        let h = quick_header(36, Mode::Versus, &[strong(Profile::ChainTsu), (SeatKind::Bot, Profile::QuadPpt, Personality::Builder, 2u8)]);
        hunt(&mut app, dir, "fx_ko", h, 60 * 600, &[2, 8, 20, 45, 90], &|_, _, e| matches!(e, Ev::Ko { .. }));
    }
    if want("six") {
        let h = quick_header(13, Mode::Versus, &[bot(Profile::QuadPpt), bot(Profile::ChainTsu), bot(Profile::CapsuleVs), bot(Profile::PanelPdp), bot(Profile::QuadPpt), bot(Profile::PanelPdp)]);
        let mut g = fx_game(&mut app, h);
        g.fast_forward(60 * 25, &app.input, &app.st, &app.synth, &mut app.r);
        g = show(&mut app, g, 0.0);
        app.r.fx.stage(&g.m, &app.r.geoms, &app.st, 7, true, 6);
        for (i, f) in [2u32, 8, 18, 30].iter().enumerate() {
            let prev = if i == 0 { 0 } else { [2u32, 8, 18, 30][i - 1] };
            for _ in prev..*f {
                app.r.fx.update(1.0 / 60.0);
            }
            g = show(&mut app, g, 0.0);
            save(&app, dir, &format!("fx_six_staged_{:02}", f));
        }
    }
    if want("variants") {
        // the same 3-chain under the settings that tone effects down
        let base = app.st.clone();
        for (name, f) in [
            ("fx_var_tournament", (|st: &mut crate::save::Settings| st.theme = crate::theme::ThemeId::Tournament) as fn(&mut crate::save::Settings)),
            ("fx_var_potato", |st| st.preset = crate::theme::Preset::Potato),
            ("fx_var_reduce_motion", |st| {
                st.reduce_motion = true;
                st.shake = false;
            }),
        ] {
            app.st = base.clone();
            f(&mut app.st);
            app.apply_settings();
            let h = quick_header(31, Mode::Versus, &[strong(Profile::ChainTsu), strong(Profile::ChainTsu)]);
            hunt(&mut app, dir, name, h, 60 * 240, &[3, 22], &|_, _, e| matches!(e, Ev::ChainStep { n } if *n >= 3));
        }
        app.st = base;
        app.apply_settings();
    }
    if want("allclear") {
        let h = quick_header(11, Mode::Versus, &[bot(Profile::QuadPpt), bot(Profile::ChainTsu)]);
        let mut g = fx_game(&mut app, h);
        g.fast_forward(60 * 6, &app.input, &app.st, &app.synth, &mut app.r);
        g = show(&mut app, g, 0.0);
        app.r.fx = crate::fx::Fx::new();
        app.r.fx.k = app.cv.k;
        app.r.fx.stage_all_clear(&g.m, &app.r.geoms, &app.st, 1);
        let mut prev = 0;
        for f in [3u32, 10, 24] {
            for _ in prev..f {
                app.r.fx.update(1.0 / 60.0);
            }
            prev = f;
            g = show(&mut app, g, 0.0);
            save(&app, dir, &format!("fx_allclear_{:02}", f));
        }
    }
}

/// The README banner: a world map, the cast lined up, the logo. 1920 x 720.
pub fn banner(path: &str) {
    let synth = Arc::new(Mutex::new(crate::audio::Synth::new()));
    let input = crate::input::Input::new(None);
    let mut app = App::new(input, synth, false);
    app.st = crate::save::Settings::default();
    app.st.benched = true;
    app.apply_settings();
    app.cv = crate::gfx::Canvas::screen(1920);
    // the band that is kept is layout y 120..600
    app.title_backdrop(4.5);
    for (id, ex, x, foot, h, flip) in [
        ("kai", "smug", 105, 690, 560, false),
        ("nami", "smile", 225, 670, 540, false),
        ("hana", "delighted", 1175, 670, 540, true),
        ("rin", "smug", 1055, 690, 560, true),
        ("mira", "delighted", 365, 740, 610, false),
        ("kaede", "smug", 915, 740, 610, true),
    ] {
        crate::assets::draw_char_ex(&mut app.cv, id, ex, x, foot, h, 255, flip);
    }
    crate::assets::logo(&mut app.cv, 640 - 255, 150, 510);
    let tag = "Four puzzle sports.  One board.  Open source.";
    let tw = crate::gfx::Canvas::text_wb(tag, 2, crate::font::Weight::Black) + 56;
    app.cv.round_rect(640 - tw / 2 - 4, 452, tw + 8, 50, 25, 0xFF3B_2A57, 255);
    app.cv.round_rect(640 - tw / 2, 456, tw, 42, 21, 0xFFFF_F6D6, 255);
    app.cv.text_weight(640 - tw / 2 + 28, 465, tag, 2, 0xFF3B_2A57, 255, crate::font::Weight::Black);
    // crop the middle band (16:6)
    let (w, h) = (app.cv.w as usize, app.cv.h as usize);
    let band = w * 6 / 16;
    let top = (h - band) / 2;
    let px: Vec<u32> = app.cv.px[top * w..(top + band) * w].to_vec();
    crate::png::write(path, w, band, &px).unwrap();
    println!("wrote {}", path);
}
