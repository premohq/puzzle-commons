//! purtris-tools: headless. Soak, determinism, replay verify, seed browser, puzzle check.
//! Usage: purtris-tools <soak|determinism|puzzles|seed|verify FILE|bench>

use purtris_bot::{bot_input, fill_bot_inputs};
use purtris_sim::replay::Replay;
use purtris_sim::*;
use std::time::Instant;

fn header_for(seed: u64, profiles: &[Profile], strength: u8) -> MatchHeader {
    let seats: Vec<(SeatKind, Profile, Personality, u8)> = profiles
        .iter()
        .enumerate()
        .map(|(i, &p)| (SeatKind::Bot, p, Personality::ALL[(i + seed as usize) % 6], strength))
        .collect();
    let mut h = quick_header(seed, Mode::Versus, &seats);
    h.opts.shared_deal = false;
    h
}

fn run(h: MatchHeader, max_ticks: u32) -> (Match, Replay) {
    let mut m = Match::new(h).expect("create");
    let mut rep = Replay::new(h);
    while m.outcome == Outcome::Running && m.tick < max_ticks {
        let mut ins = [0u16; 6];
        fill_bot_inputs(&mut m, &mut ins);
        m.step(ins);
        rep.record(&m, ins);
    }
    (m, rep)
}

fn soak() {
    let styles = [Profile::QuadPpt, Profile::ChainTsu, Profile::CapsuleVs, Profile::PanelPdp];
    let mut n = 0;
    let t0 = Instant::now();
    let mut ticks = 0u64;
    for a in styles {
        for b in styles {
            for seed in 1..4u64 {
                let (m, _) = run(header_for(seed * 1000 + n, &[a, b], 6), 60 * 60 * 4);
                ticks += m.tick as u64;
                let s0 = &m.seats[0].stats;
                let s1 = &m.seats[1].stats;
                println!(
                    "{:>15} vs {:<15} seed {:>5}: {:?} at tick {:>6}  sent {}/{}  chains {}/{}",
                    a.id(), b.id(), seed, m.outcome, m.tick, s0.sent, s1.sent, s0.max_chain, s1.max_chain
                );
                n += 1;
            }
        }
    }
    // six seats, mixed, with teams
    for seed in 0..4u64 {
        let mut h = header_for(seed + 77, &[Profile::QuadPpt, Profile::ChainTsu, Profile::CapsuleVs, Profile::PanelPdp, Profile::QuadPpt, Profile::PanelPdp], 5);
        if seed % 2 == 1 {
            for i in 0..6 {
                h.seats[i].team = 1 + (i as u8 % 2);
            }
            h.target = TargetRule::Team;
        } else {
            h.target = TargetRule::ALL[seed as usize % 3];
        }
        let (m, _) = run(h, 60 * 60 * 5);
        ticks += m.tick as u64;
        let kos: Vec<u32> = m.seats.iter().map(|s| s.kos).collect();
        println!("6-seat {:?} seed {}: {:?} at tick {} kos {:?}", h.target, seed, m.outcome, m.tick, kos);
    }
    // solo modes
    for (mode, p) in [
        (Mode::QuadSprint, Profile::QuadGuideline),
        (Mode::QuadMarathon, Profile::QuadGuideline),
        (Mode::ChainEndless, Profile::ChainTsu),
        (Mode::CapsuleLevelRun, Profile::CapsuleNes),
        (Mode::PanelEndless, Profile::PanelPdp),
    ] {
        let h = quick_header(9, mode, &[(SeatKind::Bot, p, Personality::Builder, 8)]);
        let (m, _) = run(h, 60 * 60 * 6);
        ticks += m.tick as u64;
        let score = m.seats[0].board.as_ref().unwrap().score();
        println!("{:<26} {:?} at tick {} score {}", mode.name(), m.outcome, m.tick, score);
    }
    let mut h = header_for(5, &[Profile::CapsuleNes, Profile::CapsuleNes], 6);
    h.mode = Mode::CapsuleRace;
    h.opts.capsule_level = 2;
    let (m, _) = run(h, 60 * 60 * 6);
    println!("Capsule Race: {:?} at tick {}", m.outcome, m.tick);
    let dt = t0.elapsed().as_secs_f64();
    println!("{} sim ticks in {:.2}s ({:.1} us/tick incl. bots)", ticks, dt, dt * 1e6 / ticks as f64);
}

fn determinism() {
    let h = header_for(424242, &[Profile::QuadPpt, Profile::ChainTsu, Profile::CapsuleVs, Profile::PanelPdp, Profile::ChainTsu, Profile::QuadPpt], 7);
    let (m1, rep) = run(h, 60 * 120);
    let (m2, _) = run(h, 60 * 120);
    assert_eq!(m1.checksum(), m2.checksum(), "two runs differ");
    let bytes = rep.to_bytes();
    let back = Replay::from_bytes(&bytes).expect("parse");
    let res = back.verify(&|m: &mut Match, i: usize| {
        let mut mem = std::mem::take(&mut m.bot_mem[i]);
        let b = bot_input(m, i, &mut mem);
        m.bot_mem[i] = mem;
        b
    });
    match res {
        Ok(m3) => {
            assert_eq!(m3.checksum(), m1.checksum());
            println!("determinism OK: {} ticks, checksum {:016X}, replay {} bytes", m1.tick, m1.checksum(), bytes.len());
        }
        Err(t) => panic!("replay desync at tick {}", t),
    }
    // rewind: snapshot and re-step must match
    let mut a = Match::new(h).unwrap();
    for _ in 0..600 {
        let mut ins = [0u16; 6];
        fill_bot_inputs(&mut a, &mut ins);
        a.step(ins);
    }
    let snap = a.clone();
    let mut b = snap.clone();
    for _ in 0..300 {
        let mut ins = [0u16; 6];
        fill_bot_inputs(&mut a, &mut ins);
        a.step(ins);
        let mut ins2 = [0u16; 6];
        fill_bot_inputs(&mut b, &mut ins2);
        b.step(ins2);
    }
    assert_eq!(a.checksum(), b.checksum());
    println!("rewind snapshot OK");
}

fn puzzles() {
    use purtris_sim::panel::PUZZLES;
    for (pi, pz) in PUZZLES.iter().enumerate() {
        let h = quick_header(1, Mode::PanelPuzzle(pi as u8), &[(SeatKind::Human, Profile::PanelPdp, Personality::Builder, 1)]);
        // brute force over swap positions (BFS depth = moves)
        let base = Match::new(h).unwrap();
        let mut frontier = vec![(base, Vec::<(i32, i32)>::new())];
        let mut solved = None;
        for _depth in 0..pz.moves {
            let mut next = Vec::new();
            for (m, path) in frontier {
                for y in panel::VIS0..panel::H {
                    for x in 0..panel::W - 1 {
                        let mut t = m.clone();
                        if let Some(Board::Panel(p)) = t.seats[0].board.as_mut() {
                            p.cx = x;
                            p.cy = y;
                        }
                        t.step([purtris_sim::bits::SWAP, 0, 0, 0, 0, 0]);
                        for _ in 0..400 {
                            if t.outcome != Outcome::Running {
                                break;
                            }
                            if let Some(Board::Panel(p)) = t.seats[0].board.as_ref() {
                                if p.moves_left > 0 && p.grid.iter().all(|c| c.s == panel::St::Idle) {
                                    break;
                                }
                            }
                            t.step([0; 6]);
                        }
                        let mut np = path.clone();
                        np.push((x, y));
                        if t.outcome == (Outcome::SoloDone { success: true }) {
                            solved = Some(np);
                            break;
                        }
                        if t.outcome == Outcome::Running {
                            next.push((t, np));
                        }
                    }
                    if solved.is_some() {
                        break;
                    }
                }
                if solved.is_some() {
                    break;
                }
            }
            if solved.is_some() {
                break;
            }
            frontier = next;
        }
        match solved {
            Some(path) => println!("puzzle {} '{}' solvable in {} : {:?}", pi, pz.name, pz.moves, path),
            None => {
                println!("puzzle {} '{}' NOT SOLVABLE in {} moves", pi, pz.name, pz.moves);
                std::process::exit(1);
            }
        }
    }
}

fn seed_browser(seed: u64) {
    let h = quick_header(seed, Mode::Versus, &[(SeatKind::Human, Profile::QuadPpt, Personality::Builder, 1), (SeatKind::Human, Profile::ChainTsu, Personality::Builder, 1)]);
    let m = Match::new(h).unwrap();
    if let Some(Board::Quad(q)) = &m.seats[0].board {
        let s: String = q.queue.iter().map(|&t| quad::PIECE_NAMES[t as usize]).collect();
        println!("quad.bag  : {}", s);
    }
    if let Some(Board::Chain(c)) = &m.seats[1].board {
        println!("dropset   : {:?}", c.queue);
    }
}

fn bench() {
    let h = header_for(1, &[Profile::QuadPpt, Profile::ChainTsu, Profile::CapsuleVs, Profile::PanelPdp, Profile::QuadPpt, Profile::ChainTsu], 1);
    let mut m = Match::new(h).unwrap();
    let mut worst = 0f64;
    let t0 = Instant::now();
    for _ in 0..3600 {
        let t = Instant::now();
        m.step([0; 6]);
        worst = worst.max(t.elapsed().as_secs_f64());
    }
    println!("6 mixed boards, sim only: avg {:.1} us/tick, worst {:.1} us (budget 2000 us)", t0.elapsed().as_secs_f64() * 1e6 / 3600.0, worst * 1e6);
}

fn survive() {
    for p in [Profile::QuadPpt, Profile::ChainTsu, Profile::CapsuleVs, Profile::PanelPdp] {
        for per in [Personality::Builder, Personality::Aggressor, Personality::Digger, Personality::Cleaner, Personality::Punisher, Personality::RandomLegal] {
            let mut line = format!("{:<12} {:<13}", p.id(), per.name());
            for s in [2u8, 5, 8] {
                let mut tot = 0;
                for seed in 1..4u64 {
                    let h = quick_header(seed, Mode::Training, &[(SeatKind::Bot, p, per, s)]);
                    let (m, _) = run(h, 60 * 180);
                    tot += m.tick;
                }
                line.push_str(&format!("  s{}:{:>5}s", s, tot / 180));
            }
            println!("{}", line);
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("soak") => soak(),
        Some("determinism") => determinism(),
        Some("puzzles") => puzzles(),
        Some("seed") => seed_browser(args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1)),
        Some("bench") => bench(),
        Some("survive") => survive(),
        Some("verify") => {
            let path = args.get(2).expect("file");
            let bytes = std::fs::read(path).expect("read");
            let rep = Replay::from_bytes(&bytes).expect("not a replay");
            match rep.verify(&|m: &mut Match, i: usize| {
                let mut mem = std::mem::take(&mut m.bot_mem[i]);
                let b = bot_input(m, i, &mut mem);
                m.bot_mem[i] = mem;
                b
            }) {
                Ok(m) => println!("OK: resimulates, {} ticks, checksum {:016X}", m.tick, m.checksum()),
                Err(t) => println!("DESYNC at tick {}", t),
            }
        }
        _ => {
            determinism();
            bench();
            puzzles();
            soak();
        }
    }
}
