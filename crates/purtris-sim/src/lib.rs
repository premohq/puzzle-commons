//! purtris-sim: rules, match step, attack routing, serialization, checksums, and the C ABI.
//! No window, no audio, no OS clock, no floating point, no OS randomness.

pub mod capi;
pub mod capsule;
pub mod chain;
pub mod common;
pub mod matchs;
pub mod panel;
pub mod quad;
pub mod replay;
pub mod rng;

pub use common::*;
pub use matchs::*;

pub const ENGINE_VERSION: &str = "1.0.0";
pub const SER_VERSION: u32 = 1;
/// Shared cross-path travel delay (ticks at 60 Hz).
pub const CROSS_DELAY: u32 = 30;
pub const TICK_HZ: u32 = 60;

/// Convenience: a header for a 1v1 or solo setup.
pub fn quick_header(seed: u64, mode: Mode, seats: &[(SeatKind, Profile, Personality, u8)]) -> MatchHeader {
    let mut s = [SeatCfg::default(); SEATS];
    for (i, &(kind, profile, personality, strength)) in seats.iter().enumerate().take(SEATS) {
        s[i] = SeatCfg { kind, profile, team: 0, personality, strength, bot_seed: (i as u32 + 1) * 7919 };
    }
    MatchHeader { seed, mode, target: TargetRule::Direct, seats: s, opts: Opts::default() }
}
