//! Replay: b"PURTRIS", version, MatchHeader, then six InputBits per tick.
//! Metadata chunk after the inputs: u32 length + bytes (checksums every 60 ticks, names).

use crate::common::Profile;
use crate::matchs::{Match, MatchHeader, Mode, Opts, Personality, SeatCfg, SeatKind, TargetRule, SEATS};

pub const MAGIC: &[u8; 7] = b"PURTRIS";
pub const FORMAT: u8 = 1;

#[derive(Clone, Debug)]
pub struct Replay {
    pub header: MatchHeader,
    pub inputs: Vec<[u16; SEATS]>,
    /// (tick, checksum) every 60 ticks
    pub checks: Vec<(u32, u64)>,
    pub names: Vec<String>,
    pub tas: bool,
}

struct Wr(Vec<u8>);
impl Wr {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
}
struct Rd<'a>(&'a [u8], usize);
impl Rd<'_> {
    fn u8(&mut self) -> Option<u8> {
        let v = *self.0.get(self.1)?;
        self.1 += 1;
        Some(v)
    }
    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes([self.u8()?, self.u8()?]))
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes([self.u8()?, self.u8()?, self.u8()?, self.u8()?]))
    }
    fn u64(&mut self) -> Option<u64> {
        Some(self.u32()? as u64 | ((self.u32()? as u64) << 32))
    }
}

pub fn write_header(w: &mut Vec<u8>, h: &MatchHeader) {
    let mut wr = Wr(std::mem::take(w));
    wr.u64(h.seed);
    let (a, b) = h.mode.code();
    wr.u8(a);
    wr.u8(b);
    wr.u8(TargetRule::ALL.iter().position(|&t| t == h.target).unwrap() as u8);
    for s in &h.seats {
        wr.u8(match s.kind {
            SeatKind::Closed => 0,
            SeatKind::Human => 1,
            SeatKind::Bot => 2,
        });
        wr.u8(s.profile.code());
        wr.u8(s.team);
        wr.u8(s.personality.code());
        wr.u8(s.strength);
        wr.u32(s.bot_seed);
    }
    let o = &h.opts;
    wr.u32(o.quad_level);
    wr.u8(o.chain_colors);
    wr.u32(o.capsule_level);
    wr.u8(o.capsule_speed);
    wr.u8(o.nes_seeds as u8);
    wr.u32(o.panel_level);
    wr.u8(o.panel_colors);
    wr.u8(o.virus_clear_wins as u8);
    wr.u8(o.force_cross as u8);
    wr.u8(o.shared_deal as u8);
    wr.u8(o.team_follow_split as u8);
    *w = wr.0;
}

fn read_header(r: &mut Rd) -> Option<MatchHeader> {
    let seed = r.u64()?;
    let a = r.u8()?;
    let b = r.u8()?;
    let mode = Mode::from_code(a, b);
    let target = *TargetRule::ALL.get(r.u8()? as usize)?;
    let mut seats = [SeatCfg::default(); SEATS];
    for s in seats.iter_mut() {
        s.kind = match r.u8()? {
            1 => SeatKind::Human,
            2 => SeatKind::Bot,
            _ => SeatKind::Closed,
        };
        s.profile = Profile::from_code(r.u8()?);
        s.team = r.u8()?;
        s.personality = Personality::from_code(r.u8()?);
        s.strength = r.u8()?;
        s.bot_seed = r.u32()?;
    }
    let opts = Opts {
        quad_level: r.u32()?,
        chain_colors: r.u8()?,
        capsule_level: r.u32()?,
        capsule_speed: r.u8()?,
        nes_seeds: r.u8()? != 0,
        panel_level: r.u32()?,
        panel_colors: r.u8()?,
        virus_clear_wins: r.u8()? != 0,
        force_cross: r.u8()? != 0,
        shared_deal: r.u8()? != 0,
        team_follow_split: r.u8()? != 0,
    };
    Some(MatchHeader { seed, mode, target, seats, opts })
}

impl Replay {
    pub fn new(header: MatchHeader) -> Self {
        Replay { header, inputs: Vec::new(), checks: Vec::new(), names: Vec::new(), tas: false }
    }
    pub fn record(&mut self, m: &Match, inputs: [u16; SEATS]) {
        self.inputs.push(inputs);
        if m.tick % 60 == 0 {
            self.checks.push((m.tick, m.checksum()));
        }
    }
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut v = MAGIC.to_vec();
        v.push(FORMAT);
        write_header(&mut v, &self.header);
        let mut wr = Wr(v);
        wr.u32(self.inputs.len() as u32);
        for t in &self.inputs {
            for &b in t {
                wr.u16(b);
            }
        }
        // metadata chunk
        let mut meta = Wr(Vec::new());
        meta.u8(self.tas as u8);
        meta.u32(self.checks.len() as u32);
        for &(t, c) in &self.checks {
            meta.u32(t);
            meta.u64(c);
        }
        meta.u8(self.names.len() as u8);
        for n in &self.names {
            let b = n.as_bytes();
            meta.u8(b.len().min(255) as u8);
            meta.0.extend_from_slice(&b[..b.len().min(255)]);
        }
        wr.u32(meta.0.len() as u32);
        wr.0.extend_from_slice(&meta.0);
        wr.0
    }
    pub fn from_bytes(b: &[u8]) -> Option<Replay> {
        if b.len() < 8 || &b[..7] != MAGIC || b[7] != FORMAT {
            return None;
        }
        let mut r = Rd(b, 8);
        let header = read_header(&mut r)?;
        let n = r.u32()? as usize;
        if n > 60 * 60 * 60 * 4 {
            return None;
        }
        let mut inputs = Vec::with_capacity(n);
        for _ in 0..n {
            let mut t = [0u16; SEATS];
            for v in t.iter_mut() {
                *v = r.u16()?;
            }
            inputs.push(t);
        }
        let mut rep = Replay::new(header);
        rep.inputs = inputs;
        if let Some(len) = r.u32() {
            let start = r.1;
            if start + len as usize <= b.len() {
                rep.tas = r.u8()? != 0;
                let k = r.u32()?;
                for _ in 0..k {
                    let t = r.u32()?;
                    let c = r.u64()?;
                    rep.checks.push((t, c));
                }
                if let Some(nn) = r.u8() {
                    for _ in 0..nn {
                        let l = r.u8()? as usize;
                        let s = String::from_utf8_lossy(r.0.get(r.1..r.1 + l)?).to_string();
                        r.1 += l;
                        rep.names.push(s);
                    }
                }
            }
        }
        Some(rep)
    }

    /// Resimulate with a bot function and compare checksums. Returns the first bad tick, if any.
    pub fn verify(&self, bot: &dyn Fn(&mut Match, usize) -> u16) -> Result<Match, u32> {
        let mut m = Match::new(self.header).map_err(|_| 0u32)?;
        let mut ci = 0;
        for inputs in &self.inputs {
            let mut ins = *inputs;
            for i in 0..SEATS {
                if self.header.seats[i].kind == SeatKind::Bot && m.seats[i].alive {
                    let b = bot(&mut m, i);
                    if b != ins[i] {
                        return Err(m.tick + 1);
                    }
                    ins[i] = b;
                }
            }
            m.step(ins);
            while ci < self.checks.len() && self.checks[ci].0 == m.tick {
                if self.checks[ci].1 != m.checksum() {
                    return Err(m.tick);
                }
                ci += 1;
            }
        }
        Ok(m)
    }
}
