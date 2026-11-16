//! C ABI over the same match object the client links. No globals, no static constructors.

use crate::common::Profile;
use crate::matchs::{Match, Mode, Personality, SeatKind, SEATS};

/// Create a match. `profiles` holds six profile codes; 255 = closed seat.
/// Returns null on a bad header. Free with `purtris_match_free`.
///
/// # Safety
/// `profiles` must point to six readable bytes.
#[no_mangle]
pub unsafe extern "C" fn purtris_match_new(seed: u64, profiles: *const u8) -> *mut Match {
    if profiles.is_null() {
        return std::ptr::null_mut();
    }
    let p = std::slice::from_raw_parts(profiles, SEATS);
    let seats: Vec<(SeatKind, Profile, Personality, u8)> = p
        .iter()
        .map(|&c| if c == 255 { (SeatKind::Closed, Profile::QuadPpt, Personality::Builder, 1) } else { (SeatKind::Human, Profile::from_code(c), Personality::Builder, 1) })
        .collect();
    let h = crate::quick_header(seed, Mode::Versus, &seats);
    match Match::new(h) {
        Ok(m) => Box::into_raw(Box::new(m)),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Step one tick with six bitfields.
///
/// # Safety
/// `m` must come from `purtris_match_new`; `inputs` must point to six u16.
#[no_mangle]
pub unsafe extern "C" fn purtris_match_step(m: *mut Match, inputs: *const u16) {
    if m.is_null() || inputs.is_null() {
        return;
    }
    let s = std::slice::from_raw_parts(inputs, SEATS);
    let mut a = [0u16; SEATS];
    a.copy_from_slice(s);
    (*m).step(a);
}

/// # Safety
/// `m` must come from `purtris_match_new`.
#[no_mangle]
pub unsafe extern "C" fn purtris_match_checksum(m: *const Match) -> u64 {
    if m.is_null() {
        0
    } else {
        (*m).checksum()
    }
}

/// # Safety
/// `m` must come from `purtris_match_new` and not be used afterward.
#[no_mangle]
pub unsafe extern "C" fn purtris_match_free(m: *mut Match) {
    if !m.is_null() {
        drop(Box::from_raw(m));
    }
}
