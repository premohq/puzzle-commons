//! The story script: story.txt, parsed once into the scenes the VN screen plays (story_ui.rs),
//! and `--story-check`, which checks every line against the dialogue box and renders frames.

use crate::cast::{self, Line, Scene, CAST, EXPRESSIONS};
use std::sync::OnceLock;

const SCRIPT: &str = include_str!("story.txt");

/// The scenes every story trigger names (app.rs); the check makes sure they exist.
const REQUIRED: [&str; 3] = ["prologue", "w4_fall", "epilogue"];
const PER_WORLD: [&str; 4] = ["intro", "mid", "boss", "clear"];

fn known_speaker(id: &str) -> bool {
    CAST.iter().any(|m| m.id == id) || cast::voice(id).is_some()
}

/// Parse the script. Problems are collected rather than fatal: a bad line is skipped and
/// reported, so a typo never takes the game down (the check fails on them instead).
fn parse(src: &'static str) -> (Vec<Scene>, Vec<String>) {
    let mut scenes = Vec::new();
    let mut errors = Vec::new();
    let mut cur: Option<Scene> = None;
    for (n, raw) in src.lines().enumerate() {
        let ln = n + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("SCENE ") {
            if let Some(s) = cur.take() {
                errors.push(format!("line {}: scene {} has no END", ln, s.id));
                scenes.push(s);
            }
            let mut it = rest.split_whitespace();
            let id = it.next().unwrap_or("");
            let world = it.next().and_then(|w| w.parse().ok()).unwrap_or_else(|| {
                errors.push(format!("line {}: SCENE {} needs a world number", ln, id));
                0
            });
            if scenes.iter().any(|s: &Scene| s.id == id) {
                errors.push(format!("line {}: scene {} defined twice", ln, id));
            }
            cur = Some(Scene { id, world, right: Vec::new(), lines: Vec::new() });
        } else if let Some(rest) = line.strip_prefix("RIGHT") {
            match cur.as_mut() {
                Some(s) => {
                    for w in rest.split_whitespace() {
                        if !known_speaker(w) {
                            errors.push(format!("line {}: unknown speaker {}", ln, w));
                        }
                        s.right.push(w);
                    }
                }
                None => errors.push(format!("line {}: RIGHT outside a scene", ln)),
            }
        } else if line == "END" {
            match cur.take() {
                Some(s) if s.lines.is_empty() => errors.push(format!("line {}: scene {} is empty", ln, s.id)),
                Some(s) => scenes.push(s),
                None => errors.push(format!("line {}: END outside a scene", ln)),
            }
        } else if let Some((head, text)) = line.split_once('|') {
            let mut it = head.split_whitespace();
            let (who, expr) = (it.next().unwrap_or(""), it.next().unwrap_or("normal"));
            let text = text.trim();
            if !known_speaker(who) {
                errors.push(format!("line {}: unknown speaker {}", ln, who));
            } else if !EXPRESSIONS.contains(&expr) {
                errors.push(format!("line {}: unknown expression {}", ln, expr));
            } else if text.is_empty() {
                errors.push(format!("line {}: empty line", ln));
            } else if let Some(s) = cur.as_mut() {
                s.lines.push(Line { who, expr, text });
            } else {
                errors.push(format!("line {}: dialogue outside a scene", ln));
            }
        } else {
            errors.push(format!("line {}: not understood: {}", ln, line));
        }
    }
    if let Some(s) = cur {
        errors.push(format!("scene {} has no END", s.id));
        scenes.push(s);
    }
    (scenes, errors)
}

pub fn scenes() -> &'static [Scene] {
    static S: OnceLock<Vec<Scene>> = OnceLock::new();
    S.get_or_init(|| {
        let (s, errors) = parse(SCRIPT);
        for e in &errors {
            eprintln!("story.txt {}", e);
        }
        s
    })
}

/// Typewriter: how many characters of `text` show after `secs`. Sentences end with a beat,
/// commas with a breath, so lines read the way they would be said.
pub fn reveal(text: &str, secs: f32) -> usize {
    const CHAR: f32 = 1.0 / 52.0;
    let mut t = 0.0;
    let chars: Vec<char> = text.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        t += CHAR;
        if t > secs {
            return i;
        }
        let next_space = chars.get(i + 1).map_or(false, |n| *n == ' ');
        t += match c {
            '.' | '!' | '?' if next_space => 0.22,
            ',' | ':' | ';' if next_space => 0.08,
            _ => 0.0,
        };
    }
    chars.len()
}

/// `--story-check DIR WIDTH [ids...]`: parse errors, missing trigger scenes, lines that overflow
/// the dialogue box, node cards that overflow, node casts that don't match their seats. Every
/// line of each scene id given is rendered to DIR. Returns the process exit code.
pub fn check(dir: &str, width: i32, ids: &[String]) -> i32 {
    use crate::app::{App, Screen};
    use crate::content::WORLDS;
    use crate::gfx::wrap;
    let (parsed, mut errors) = parse(SCRIPT);
    let mut ids_needed: Vec<String> = REQUIRED.iter().map(|s| s.to_string()).collect();
    for w in 0..5 {
        ids_needed.extend(PER_WORLD.iter().map(|k| format!("w{}_{}", w, k)));
    }
    for id in &ids_needed {
        if !parsed.iter().any(|s| s.id == id) {
            errors.push(format!("missing scene {}", id));
        }
    }
    let mut lines = 0;
    // every line must fit with the default name and with a long, wide one, as a girl and a boy
    let variants = [("Mira", false), ("Mira", true), ("Wwwwwwwwww", false), ("Wwwwwwwwww", true)];
    let fits = |text: &str, maxw: i32, max_rows: usize| -> Option<String> {
        for (name, boy) in variants {
            cast::set_hero(name);
            cast::set_hero_boy(boy);
            let said = cast::say(text).into_owned();
            if said.contains('{') || said.contains('}') {
                return Some(format!("unknown token in: {}", said));
            }
            let rows = wrap(&said, maxw, 3).len();
            if rows > max_rows {
                return Some(format!("{} rows (max {}) as {}: {}", rows, max_rows, name, said));
            }
        }
        None
    };
    for s in &parsed {
        if s.world > 4 {
            errors.push(format!("{}: world {} out of range", s.id, s.world));
        }
        for (k, l) in s.lines.iter().enumerate() {
            lines += 1;
            if let Some(e) = fits(l.text, 1090, 3) {
                errors.push(format!("{} line {}: {}", s.id, k + 1, e));
            }
            if l.text.contains('\u{2014}') {
                errors.push(format!("{} line {}: em dash", s.id, k + 1));
            }
        }
    }
    for (wi, w) in WORLDS.iter().enumerate() {
        for (ni, n) in w.nodes.iter().enumerate() {
            if let Some(e) = fits(n.line, 960, 2) {
                errors.push(format!("node {}-{} card: {}", wi, ni, e));
            }
            let seats = n.level.allies.len() + n.level.foes.len();
            if n.who.len() != seats {
                errors.push(format!("node {}-{}: {} cast ids for {} bot seats", wi, ni, n.who.len(), seats));
            }
            for id in n.who {
                if !CAST.iter().any(|m| m.id == *id) {
                    errors.push(format!("node {}-{}: unknown cast id {}", wi, ni, id));
                }
            }
        }
    }
    let card = crate::save::Card::load();
    cast::set_hero(&card.hero);
    cast::set_hero_boy(card.hero_boy);
    println!("{} scenes, {} lines", parsed.len(), lines);
    for e in &errors {
        println!("ERROR {}", e);
    }
    if !ids.is_empty() {
        let _ = std::fs::create_dir_all(dir);
        let synth = std::sync::Arc::new(std::sync::Mutex::new(crate::audio::Synth::new()));
        let mut app = App::new(crate::input::Input::new(None), synth, false);
        app.st = crate::save::Settings::default();
        app.st.benched = true;
        app.apply_settings();
        app.cv = crate::gfx::Canvas::screen(width);
        // "+boy", "+girl" and "+name=X" set how the courier is drawn and named for the frames
        // after them; "+namescreen" renders the naming screen
        for id in ids {
            if id == "+boy" || id == "+girl" {
                cast::set_hero_boy(id == "+boy");
                app.card.hero_boy = id == "+boy";
                continue;
            }
            if let Some(n) = id.strip_prefix("+name=") {
                cast::set_hero(n);
                app.card.hero = n.to_string();
                continue;
            }
            if id == "+namescreen" {
                app.screen = Screen::NameHero { typed: cast::hero(), then: Box::new(Screen::Title) };
                app.frame(None, 1.0 / 60.0);
                let p = format!("{}/namescreen_{}.png", dir, if cast::hero_boy() { "boy" } else { "girl" });
                crate::png::write(&p, app.cv.w as usize, app.cv.h as usize, &app.cv.px).unwrap();
                println!("wrote {}", p);
                continue;
            }
            let Some(sc) = cast::scene(id) else {
                println!("no scene {}", id);
                continue;
            };
            for line in 0..sc.lines.len() {
                app.screen = Screen::Scene { id: sc.id, line, t: 30.0, then: Box::new(Screen::Title) };
                app.frame(None, 1.0 / 60.0);
                let p = format!("{}/{}_{:02}.png", dir, sc.id, line + 1);
                crate::png::write(&p, app.cv.w as usize, app.cv.h as usize, &app.cv.px).unwrap();
            }
            println!("wrote {} frames of {}", sc.lines.len(), sc.id);
        }
    }
    if errors.is_empty() {
        println!("story OK");
        0
    } else {
        1
    }
}
