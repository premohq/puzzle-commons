//! Named, seeded random streams. Integer only. No OS randomness, no clock.

pub const SHARED_SCOPE: u16 = 0xFFFF;
pub const PER_SEAT: u16 = 0;

#[inline]
pub fn fmix32(mut h: u32) -> u32 {
    h ^= h >> 16;
    h = h.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 13;
    h = h.wrapping_mul(0xc2b2_ae35);
    h ^= h >> 16;
    h
}

pub fn hash_str(s: &str, seed: u32) -> u32 {
    let mut h = seed ^ 0x811c_9dc5;
    for b in s.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    fmix32(h)
}

/// stream_seed = H(le(match_seed) || utf8(stream_name) || le(mix_in))
pub fn stream_seed(match_seed: u64, name: &str, mix_in: u16) -> u32 {
    let folded = (match_seed as u32) ^ fmix32((match_seed >> 32) as u32 ^ 0x5bd1_e995);
    fmix32(hash_str(name, folded) ^ ((mix_in as u32) + 1).wrapping_mul(0x9E37_79B1))
}

/// Design-default step (splitmix32 style). Not a console generator.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Rng {
    pub s: u32,
}

impl Rng {
    pub fn new(seed: u32) -> Self {
        Rng { s: seed }
    }
    pub fn stream(match_seed: u64, name: &str, mix_in: u16) -> Self {
        Rng::new(stream_seed(match_seed, name, mix_in))
    }
    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        self.s = self.s.wrapping_add(0x9E37_79B9);
        fmix32(self.s)
    }
    /// Uniform in [0, n) by rejection sampling. Never plain modulo.
    pub fn below(&mut self, n: u32) -> u32 {
        if n <= 1 {
            return 0;
        }
        let zone = (u32::MAX / n) * n;
        loop {
            let v = self.next_u32();
            if v < zone {
                return v % n;
            }
        }
    }
    pub fn shuffle<T>(&mut self, v: &mut [T]) {
        let mut i = v.len();
        while i > 1 {
            let j = self.below(i as u32) as usize;
            i -= 1;
            v.swap(i, j);
        }
    }
    /// Percent chance (0..=100), deterministic.
    pub fn chance(&mut self, pct: u32) -> bool {
        self.below(100) < pct
    }
}

/// 64-bit FNV-1a, used for state checksums.
#[derive(Clone, Copy)]
pub struct Hasher64(pub u64);
impl Default for Hasher64 {
    fn default() -> Self {
        Hasher64(0xcbf2_9ce4_8422_2325)
    }
}
impl Hasher64 {
    #[inline]
    pub fn byte(&mut self, b: u8) {
        self.0 ^= b as u64;
        self.0 = self.0.wrapping_mul(0x0000_0100_0000_01B3);
    }
    pub fn bytes(&mut self, bs: &[u8]) {
        for &b in bs {
            self.byte(b);
        }
    }
    pub fn u32(&mut self, v: u32) {
        self.bytes(&v.to_le_bytes());
    }
    pub fn u64(&mut self, v: u64) {
        self.bytes(&v.to_le_bytes());
    }
    pub fn i32(&mut self, v: i32) {
        self.bytes(&v.to_le_bytes());
    }
}

/// NES 16-bit LFSR step, as written in the design doc.
pub fn nes_lfsr_step(seed: u16) -> u16 {
    let feedback = ((seed >> 9) ^ (seed >> 1)) & 1;
    (seed >> 1) | (feedback << 15)
}
