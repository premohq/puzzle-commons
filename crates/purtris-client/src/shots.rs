//! Headless screenshots of real engine frames: `dr-purtris --shots DIR`.

use crate::app::{App, Screen};
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
        (
            "scene",
            Box::new(|| {
                let mut v = crate::scene_ui::SceneView::new(cast_id("w0_intro"), Screen::Title);
                v.seek(2, 0.6, 12.0);
                Screen::Scene(Box::new(v))
            }),
        ),
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
        // a character playing its 3D idle clip (two blended frames), menu and match size
        for (h, name) in [(600, "clip char 600 (anim)"), (1130, "clip char 1130 (anim)")] {
            crate::anim::draw(cv, "bench", "kaede", crate::anim::Mood::Idle, 900, 700, h, 255, true);
            std::thread::sleep(std::time::Duration::from_millis(300));
            time(name, cv, &|cv| crate::anim::draw(cv, "bench", "kaede", crate::anim::Mood::Idle, 900, 700, h, 255, true));
        }
        time("grid 30 lines", cv, &|cv| {
            for i in 0..30 {
                cv.rect_a(308 + i * 9, 108, 1, 520, 0xFF334466, 70);
            }
        });
    }
    for (name, mk) in screens.iter() {
        app.screen = mk();
        // settle (story scenes slide their cast in and light it on the first frames)
        for _ in 0..30 {
            app.frame(None, 0.016);
        }
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

/// Story scene frames: `--scene-shots DIR WIDTH`.
pub fn scenes(dir: &str, width: i32) {
    let _ = std::fs::create_dir_all(dir);
    let synth = Arc::new(Mutex::new(crate::audio::Synth::new()));
    let input = crate::input::Input::new(None);
    let mut app = App::new(input, synth, false);
    app.st = crate::save::Settings::default();
    app.st.benched = true;
    app.apply_settings();
    app.cv = crate::gfx::Canvas::screen(width);
    // (file, scene, line, seconds into the line, seconds into the scene)
    let frames: &[(&str, &str, usize, f32, f32)] = &[
        ("s01_card_drop", "w0_intro", 0, 0.0, 0.62),
        ("s02_card_full", "w0_intro", 0, 0.0, 1.5),
        ("s03_card_out", "w0_intro", 0, 0.0, 2.15),
        ("s04_first_line_typing", "w0_intro", 0, 0.9, 3.0),
        ("s05_line_done", "w0_intro", 1, 6.0, 9.0),
        ("s06_three_on_stage", "w0_intro", 2, 0.5, 12.0),
        ("s07_prologue_dusk", "prologue", 3, 6.0, 9.0),
        ("s08_valley_mid", "w1_mid", 3, 1.2, 9.0),
        ("s09_moon_night", "w2_mid", 4, 6.0, 9.0),
        ("s10_bazaar", "w3_intro", 2, 6.0, 9.0),
        ("s11_cinder_duel", "w4_boss", 1, 6.0, 9.0),
        ("s12_crowd", "w4_clear", 9, 6.0, 15.0),
        ("s13_epilogue", "epilogue", 6, 6.0, 15.0),
    ];
    for &(name, id, line, lt, st) in frames {
        // play the scene up to `line` so the stage has settled like it would in the game
        let mut v = crate::scene_ui::SceneView::new(cast_id(id), Screen::Title);
        v.seek(line, lt, st);
        app.screen = Screen::Scene(Box::new(v));
        for _ in 0..40 {
            app.frame(None, 1.0 / 60.0);
            if let Screen::Scene(v) = &mut app.screen {
                v.seek(line, lt, st);
            }
        }
        app.frame(None, 1.0 / 60.0);
        save(&app, dir, name);
    }
    // the expression change mid-hop, and the backlog
    let mut v = crate::scene_ui::SceneView::new(cast_id("w0_intro"), Screen::Title);
    v.seek(2, 3.0, 12.0);
    app.screen = Screen::Scene(Box::new(v));
    for _ in 0..30 {
        app.frame(None, 1.0 / 60.0);
    }
    if let Screen::Scene(v) = &mut app.screen {
        v.seek(3, 0.0, 13.0);
    }
    for _ in 0..7 {
        app.frame(None, 1.0 / 60.0);
    }
    save(&app, dir, "s14_expression_hop");
    if let Screen::Scene(v) = &mut app.screen {
        v.seek(6, 6.0, 20.0);
        v.open_log();
    }
    app.frame(None, 1.0 / 60.0);
    save(&app, dir, "s15_backlog");
}

fn cast_id(id: &str) -> &'static str {
    crate::cast::scene(id).or_else(|| crate::cast::scene("prologue")).map(|s| s.id).unwrap_or("prologue")
}

/// The cast under every world's light: `--light-shots DIR WIDTH`.
pub fn lights(dir: &str, width: i32) {
    use crate::light::{self, Mood, Rig};
    let _ = std::fs::create_dir_all(dir);
    let maps = crate::assets::load_maps();
    let mut cv = crate::gfx::Canvas::screen(width);
    let configs: Vec<(String, Option<usize>, Rig)> = std::iter::once(("studio".to_string(), None, Rig::STUDIO))
        .chain((0..5).flat_map(|w| [Mood::Day, Mood::Dusk, Mood::Night, Mood::Duel, Mood::Golden].into_iter().map(move |m| (format!("w{}_{:?}", w, m).to_lowercase(), Some(w), light::world(w, m)))))
        .collect();
    for (name, w, rig) in configs {
        match w {
            Some(w) => {
                let m = crate::assets::resample(&maps[w].image, cv.w, cv.h);
                cv.copy_from(&m);
                let mood = [Mood::Day, Mood::Dusk, Mood::Night, Mood::Duel, Mood::Golden].into_iter().find(|m| name.ends_with(&format!("{:?}", m).to_lowercase())).unwrap();
                let (c, a, _) = light::grade(w, mood);
                cv.rect_a(0, 0, 1280, 720, c, a);
            }
            None => cv.vgrad(0, 0, 1280, 720, 0xFF30_3850, 0xFF14_1828),
        }
        light::set(rig);
        crate::assets::draw_char_ex(&mut cv, "mira", "smile", 330, 1240, 1160, 255, false);
        crate::assets::draw_char_ex(&mut cv, "kaede", "smug", 950, 1240, 1160, 255, true);
        crate::assets::draw_portrait(&mut cv, "kai", "smile", 540, 380, 200, 255, false);
        let p = format!("{}/{}.png", dir, name);
        crate::png::write(&p, cv.w as usize, cv.h as usize, &cv.px).unwrap();
    }
    println!("wrote {}", dir);
}

/// `--anim-shots DIR [W]`: the live character animation (anim.rs) over three seconds: idle,
/// expression changes with their reactions, talking, match one-shots. Writes every frame
/// (anim_NNN.png) and a contact sheet of every 8th (anim_sheet.png).
pub fn anim_shots(dir: &str, width: i32) {
    use crate::puppet::{begin_frame, react, talk, React};
    let _ = std::fs::create_dir_all(dir);
    let mut cv = crate::gfx::Canvas::screen(width);
    // (id, flip, cx, expression changes at times)
    let cast: [(&str, bool, i32, &[(f32, &str)]); 4] = [
        ("kaede", false, 190, &[(0.0, "smile"), (0.6, "delighted"), (1.6, "angry"), (2.3, "shocked")]),
        ("mira", false, 500, &[(0.0, "normal"), (1.2, "smile"), (2.4, "sad")]),
        ("nami", true, 800, &[(0.0, "smug"), (1.0, "p-rival"), (2.0, "sleepy")]),
        ("hana", true, 1100, &[(0.0, "smile")]),
    ];
    let fps = 30.0;
    let n = 96;
    let mut frames = Vec::new();
    for f in 0..n {
        let t = 1.0 + f as f32 / fps;
        let tt = t - 1.0;
        begin_frame(t, false);
        if (0.3..1.1).contains(&tt) {
            talk("mira");
        }
        if f == 40 {
            react("hana", React::Pump);
        }
        if f == 60 {
            react("hana", React::Flinch);
        }
        if f == 75 {
            react("hana", React::Cheer);
        }
        cv.clear(0xFF2A_3350);
        cv.rect(0, 600, 1280, 120, 0xFF1D_2438);
        for (id, flip, cx, plan) in cast.iter() {
            let ex = plan.iter().rev().find(|(at, _)| tt >= *at).map(|p| p.1).unwrap_or("smile");
            crate::assets::draw_char_ex(&mut cv, id, ex, *cx, 690, 600, 255, *flip);
        }
        let p = format!("{}/anim_{:03}.png", dir, f);
        crate::png::write(&p, cv.w as usize, cv.h as usize, &cv.px).unwrap();
        if f % 8 == 0 {
            frames.push(crate::assets::resample(&cv, cv.w / 3, cv.h / 3));
        }
    }
    let (fw, fh) = (frames[0].w, frames[0].h);
    let cols = 4;
    let rows = frames.len().div_ceil(cols);
    let mut sheet = vec![0xFF00_0000u32; (fw * cols as i32 * fh * rows as i32) as usize];
    for (k, fr) in frames.iter().enumerate() {
        let (ox, oy) = ((k % cols) as i32 * fw, (k / cols) as i32 * fh);
        for y in 0..fh {
            let d = ((oy + y) * fw * cols as i32 + ox) as usize;
            sheet[d..d + fw as usize].copy_from_slice(&fr.px[(y * fw) as usize..((y + 1) * fw) as usize]);
        }
    }
    let p = format!("{}/anim_sheet.png", dir);
    crate::png::write(&p, (fw * cols as i32) as usize, (fh * rows as i32) as usize, &sheet).unwrap();
    println!("wrote {} frames and {}", n, p);
    // a 1v1 with the characters beside the boards, through the real match screen: they react to
    // big clears and garbage, and to winning and losing
    let synth = Arc::new(Mutex::new(crate::audio::Synth::new()));
    let mut app = App::new(crate::input::Input::new(None), synth, false);
    app.st = crate::save::Settings::default();
    app.st.benched = true;
    app.st.characters = true;
    app.st.chars_versus = true;
    app.apply_settings();
    app.cv = crate::gfx::Canvas::screen(width);
    let h = quick_header(5, Mode::Versus, &[bot(Profile::QuadPpt), bot(Profile::QuadPpt)]);
    let mut g = Game::new(h, [None; 6], Default::default(), Origin::Replay, true).unwrap();
    g.chars = [crate::cast::CAST.iter().position(|m| m.id == "kai").unwrap_or(1), crate::cast::CAST.iter().position(|m| m.id == "mira").unwrap_or(0), 0, 0, 0, 0];
    g.countdown = 0;
    app.screen = Screen::Play(Box::new(g));
    let secs: usize = std::env::var("PC_ANIM_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(20);
    for f in 0..60 * secs {
        let t0 = std::time::Instant::now();
        app.frame(None, 1.0 / 60.0);
        if let Some(rest) = std::time::Duration::from_millis(16).checked_sub(t0.elapsed()) {
            std::thread::sleep(rest);
        }
        if f % 30 == 0 {
            save(&app, dir, &format!("match_{:04}", f));
        }
        if !matches!(app.screen, Screen::Play(_)) {
            save(&app, dir, &format!("match_end_{:04}", f));
            break;
        }
    }
    println!("puppet stats (characters, reactions): {:?}", crate::puppet::stats());
    // the main menu, paced in real time: its host plays her idle clip
    app.screen = Screen::Main(ListState::default());
    for f in 0..60 * 4 {
        let t0 = std::time::Instant::now();
        app.frame(None, 1.0 / 60.0);
        if let Some(rest) = std::time::Duration::from_millis(16).checked_sub(t0.elapsed()) {
            std::thread::sleep(rest);
        }
        if f % 20 == 0 {
            save(&app, dir, &format!("menu_{:04}", f));
        }
    }
}
