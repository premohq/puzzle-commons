//! Scripted input for live testing (`PC_SCRIPT=file`). Each line: `<seconds> <command> [args]`
//!   key NAME          press a key once (names as in `key_from_name`, e.g. Enter, Left, Z)
//!   hold NAME SECS    hold a key down
//!   shot FILE.png     save the frame just drawn
//!   quit              close the game
//! Times are seconds since launch. Lines starting with # are comments.

use crate::input::key_from_name;
use minifb::Key;

pub enum Cmd {
    Key(Key),
    Hold(Key, f32),
    Shot(String),
    Quit,
}

pub struct Script {
    cmds: Vec<(f32, Cmd)>,
    next: usize,
    held: Vec<(Key, f32)>,
    pub t: f32,
}

impl Script {
    pub fn from_env() -> Option<Script> {
        let path = std::env::var_os("PC_SCRIPT")?;
        let text = std::fs::read_to_string(path).ok()?;
        let mut cmds = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let p: Vec<&str> = line.split_whitespace().collect();
            let Some(t) = p.first().and_then(|v| v.parse::<f32>().ok()) else { continue };
            let cmd = match (p.get(1).copied(), p.get(2).copied(), p.get(3).copied()) {
                (Some("key"), Some(k), _) => key_from_name(k).map(Cmd::Key),
                (Some("hold"), Some(k), Some(d)) => key_from_name(k).map(|k| Cmd::Hold(k, d.parse().unwrap_or(0.2))),
                (Some("shot"), Some(f), _) => Some(Cmd::Shot(f.to_string())),
                (Some("quit"), _, _) => Some(Cmd::Quit),
                _ => None,
            };
            if let Some(c) = cmd {
                cmds.push((t, c));
            } else {
                eprintln!("PC_SCRIPT: skipped line: {}", line);
            }
        }
        cmds.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        Some(Script { cmds, next: 0, held: Vec::new(), t: 0.0 })
    }

    /// Advance; returns (keys pressed this frame, keys held, shots to save, quit).
    pub fn step(&mut self, dt: f32) -> (Vec<Key>, Vec<Key>, Vec<String>, bool) {
        self.t += dt;
        let (mut pressed, mut shots, mut quit) = (Vec::new(), Vec::new(), false);
        while self.next < self.cmds.len() && self.cmds[self.next].0 <= self.t {
            match &self.cmds[self.next].1 {
                Cmd::Key(k) => pressed.push(*k),
                Cmd::Hold(k, d) => {
                    pressed.push(*k);
                    self.held.push((*k, self.t + d));
                }
                Cmd::Shot(f) => shots.push(f.clone()),
                Cmd::Quit => quit = true,
            }
            self.next += 1;
        }
        let t = self.t;
        self.held.retain(|(_, until)| *until > t);
        let mut held: Vec<Key> = self.held.iter().map(|(k, _)| *k).collect();
        held.extend(pressed.iter().copied());
        (pressed, held, shots, quit)
    }
}
