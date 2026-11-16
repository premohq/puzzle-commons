//! Audio: an original software synth and mixer, plus a tiny native backend per OS
//! (ALSA via dlopen on Linux, waveOut on Windows, AudioQueue on macOS). No audio crates.
//! Sounds fire from sim events and are short and dry, so a chain can be counted by ear.

use std::sync::{Arc, Mutex};

pub const SR: u32 = 48000;

#[derive(Clone, Copy, PartialEq)]
pub enum Wave {
    Sine,
    Square,
    Saw,
    Tri,
    Noise,
}

#[derive(Clone, Copy)]
struct Voice {
    wave: Wave,
    f0: f32,
    f1: f32,
    t: f32,
    dur: f32,
    attack: f32,
    vol: f32,
    pan: f32,
    phase: f32,
    noise: u32,
    music: bool,
    delay: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(dead_code)] // Rise is reserved for the Panel stack-raise cue
pub enum Sfx {
    Move,
    Rotate,
    Lock,
    HardDrop,
    Hold,
    Clear(u32),
    Chain(u32),
    Garbage,
    Attack,
    Ko,
    AllClear,
    Swap,
    Virus,
    Danger,
    Menu,
    Select,
    Back,
    Win,
    Lose,
    Count,
    Go,
    Rise,
}

/// Scene keys: root frequency and a chord progression (semitones from root, minor flag).
const SCENES: [(f32, f32, [(i32, bool); 4]); 5] = [
    (220.0, 104.0, [(0, true), (-4, false), (3, false), (-2, false)]),
    (196.0, 96.0, [(0, false), (-3, true), (5, false), (7, false)]),
    (246.94, 112.0, [(0, true), (5, true), (-2, false), (3, false)]),
    (174.61, 100.0, [(0, false), (4, true), (-3, true), (5, false)]),
    (207.65, 120.0, [(0, true), (-2, false), (-4, false), (-5, false)]),
];

pub struct Synth {
    voices: Vec<Voice>,
    pub sfx_vol: f32,
    pub music_vol: f32,
    pub crt: bool,
    duck: f32,
    // music
    pub music_on: bool,
    scene: usize,
    step_t: f32,
    step: usize,
    layer: [f32; 5],
    target: [f32; 5],
    rng: u32,
    pub calm_only: bool,
}

fn semis(root: f32, s: i32) -> f32 {
    root * 2f32.powf(s as f32 / 12.0)
}

impl Synth {
    pub fn new() -> Self {
        Synth {
            voices: Vec::with_capacity(96),
            sfx_vol: 0.7,
            music_vol: 0.5,
            crt: false,
            duck: 1.0,
            music_on: true,
            scene: 0,
            step_t: 0.0,
            step: 0,
            layer: [1.0, 1.0, 0.0, 0.0, 0.0],
            target: [1.0, 1.0, 0.0, 0.0, 0.0],
            rng: 0x1234_5678,
            calm_only: false,
        }
    }

    fn voice(&mut self, wave: Wave, f0: f32, f1: f32, dur: f32, vol: f32, music: bool) {
        self.voice_d(wave, f0, f1, dur, vol, music, 0.0, 0.0);
    }
    #[allow(clippy::too_many_arguments)]
    fn voice_d(&mut self, wave: Wave, f0: f32, f1: f32, dur: f32, vol: f32, music: bool, delay: f32, pan: f32) {
        if self.voices.len() >= 96 {
            // steal the oldest music voice first
            if let Some(i) = self.voices.iter().position(|v| v.music) {
                self.voices.swap_remove(i);
            } else {
                return;
            }
        }
        let wave = if self.crt && wave != Wave::Noise && !music { Wave::Square } else { wave };
        self.rng = self.rng.wrapping_mul(1664525).wrapping_add(1013904223);
        self.voices.push(Voice {
            wave,
            f0,
            f1,
            t: 0.0,
            dur,
            attack: if music { 0.01 } else { 0.002 },
            vol,
            pan,
            phase: 0.0,
            noise: self.rng | 1,
            music,
            delay,
        });
    }

    pub fn play(&mut self, s: Sfx) {
        use Wave::*;
        match s {
            Sfx::Move => self.voice(Square, 1400.0, 1300.0, 0.018, 0.05, false),
            Sfx::Rotate => self.voice(Tri, 900.0, 1250.0, 0.035, 0.12, false),
            Sfx::Lock => {
                self.voice(Sine, 180.0, 90.0, 0.08, 0.35, false);
                self.voice(Noise, 0.0, 0.0, 0.03, 0.08, false);
            }
            Sfx::HardDrop => {
                self.voice(Sine, 220.0, 60.0, 0.12, 0.45, false);
                self.voice(Noise, 0.0, 0.0, 0.06, 0.15, false);
            }
            Sfx::Hold => self.voice(Tri, 600.0, 900.0, 0.06, 0.15, false),
            Sfx::Clear(p) => {
                let base = 523.25 * 2f32.powf((p.min(10) as f32) / 12.0);
                for (i, m) in [1.0f32, 1.26, 1.5].iter().enumerate() {
                    self.voice_d(Tri, base * m, base * m, 0.28, 0.14, false, i as f32 * 0.025, 0.0);
                }
            }
            Sfx::Chain(n) => {
                // rising, dry, countable
                let f = 392.0 * 2f32.powf((n.min(16) as f32 * 2.0) / 12.0);
                self.voice(Square, f, f, 0.12, 0.16, false);
                self.voice(Sine, f * 2.0, f * 2.0, 0.2, 0.12, false);
                self.duck = 0.4;
            }
            Sfx::Garbage => {
                self.voice(Noise, 0.0, 0.0, 0.12, 0.2, false);
                self.voice(Sine, 110.0, 55.0, 0.18, 0.4, false);
            }
            Sfx::Attack => self.voice(Saw, 300.0, 1200.0, 0.16, 0.1, false),
            Sfx::Ko => {
                self.voice(Saw, 600.0, 80.0, 0.6, 0.25, false);
                self.voice(Noise, 0.0, 0.0, 0.4, 0.15, false);
            }
            Sfx::AllClear => {
                for i in 0..6 {
                    let f = semis(523.25, [0, 4, 7, 12, 16, 19][i]);
                    self.voice_d(Tri, f, f, 0.25, 0.14, false, i as f32 * 0.06, 0.0);
                }
            }
            Sfx::Swap => self.voice(Square, 880.0, 990.0, 0.025, 0.07, false),
            Sfx::Rise => {}
            Sfx::Virus => {
                self.voice(Sine, 1568.0, 2093.0, 0.12, 0.12, false);
                self.voice_d(Sine, 2093.0, 2637.0, 0.12, 0.1, false, 0.05, 0.0);
            }
            Sfx::Danger => self.voice(Square, 220.0, 220.0, 0.1, 0.08, false),
            Sfx::Menu => self.voice(Tri, 660.0, 660.0, 0.03, 0.1, false),
            Sfx::Select => {
                self.voice(Tri, 660.0, 660.0, 0.05, 0.12, false);
                self.voice_d(Tri, 990.0, 990.0, 0.08, 0.12, false, 0.05, 0.0);
            }
            Sfx::Back => self.voice(Tri, 520.0, 380.0, 0.08, 0.12, false),
            Sfx::Win => {
                for i in 0..5 {
                    let f = semis(392.0, [0, 4, 7, 12, 16][i]);
                    self.voice_d(Square, f, f, 0.3, 0.1, false, i as f32 * 0.09, 0.0);
                }
            }
            Sfx::Lose => {
                for i in 0..4 {
                    let f = semis(392.0, [7, 3, 0, -5][i]);
                    self.voice_d(Tri, f, f, 0.35, 0.14, false, i as f32 * 0.14, 0.0);
                }
            }
            Sfx::Count => self.voice(Square, 440.0, 440.0, 0.12, 0.12, false),
            Sfx::Go => self.voice(Square, 880.0, 880.0, 0.3, 0.14, false),
        }
    }

    pub fn set_scene(&mut self, s: usize) {
        self.scene = s % SCENES.len();
    }
    /// Stems: 0 pad, 1 bass always; 2 drums on streaks, 3 arp on chains, 4 danger.
    pub fn set_intensity(&mut self, drums: bool, arp: bool, danger: bool) {
        self.target[2] = if drums && !self.calm_only { 1.0 } else { 0.0 };
        self.target[3] = if arp && !self.calm_only { 1.0 } else { 0.0 };
        self.target[4] = if danger { 1.0 } else { 0.0 };
    }

    fn music_step(&mut self) {
        let (root, _bpm, prog) = SCENES[self.scene];
        let bar = (self.step / 16) % 4;
        let s = self.step % 16;
        let (off, minor) = prog[bar];
        let chord = [0, if minor { 3 } else { 4 }, 7];
        let r = semis(root, off);
        let lw = self.layer;
        let pad_wave = if self.crt { Wave::Square } else { Wave::Tri };
        if s == 0 && lw[0] > 0.01 {
            for (k, &c) in chord.iter().enumerate() {
                let f = semis(r, c);
                self.voice_d(pad_wave, f, f, 2.2, 0.035 * lw[0], true, 0.0, if k == 0 { -0.3 } else { 0.3 });
            }
        }
        if lw[1] > 0.01 && [0, 6, 8, 14].contains(&s) {
            let f = r / 2.0;
            self.voice(Wave::Tri, f, f, 0.22, 0.16 * lw[1], true);
        }
        if lw[2] > 0.01 {
            if s % 8 == 0 {
                self.voice(Wave::Sine, 150.0, 45.0, 0.16, 0.4 * lw[2], true);
            }
            if s % 8 == 4 {
                self.voice(Wave::Noise, 0.0, 0.0, 0.1, 0.12 * lw[2], true);
            }
            if s % 2 == 0 {
                self.voice(Wave::Noise, 0.0, 0.0, 0.02, 0.05 * lw[2], true);
            }
        }
        if lw[3] > 0.01 {
            let pattern = [0, 1, 2, 1, 0, 2, 1, 2];
            let f = semis(r * 2.0, chord[pattern[s % 8]] + if s >= 8 { 12 } else { 0 });
            self.voice(if self.crt { Wave::Square } else { Wave::Sine }, f, f, 0.1, 0.05 * lw[3], true);
        }
        if lw[4] > 0.01 && s % 4 == 0 {
            self.voice(Wave::Saw, 55.0, 52.0, 0.15, 0.08 * lw[4], true);
        }
        self.step += 1;
    }

    /// Render interleaved stereo f32.
    pub fn render(&mut self, out: &mut [f32]) {
        let dt = 1.0 / SR as f32;
        let frames = out.len() / 2;
        let step_len = 60.0 / SCENES[self.scene].1 / 4.0;
        for f in 0..frames {
            if self.music_on {
                self.step_t += dt;
                if self.step_t >= step_len {
                    self.step_t -= step_len;
                    for i in 0..5 {
                        self.layer[i] += (self.target[i] - self.layer[i]) * 0.25;
                    }
                    self.music_step();
                }
            }
            self.duck += (1.0 - self.duck) * 0.00012;
            let mut l = 0.0f32;
            let mut r = 0.0f32;
            for v in self.voices.iter_mut() {
                if v.delay > 0.0 {
                    v.delay -= dt;
                    continue;
                }
                let p = (v.t / v.dur).min(1.0);
                let freq = v.f0 + (v.f1 - v.f0) * p;
                v.phase += freq * dt;
                if v.phase >= 1.0 {
                    v.phase -= v.phase.floor();
                }
                let s = match v.wave {
                    Wave::Sine => (v.phase * std::f32::consts::TAU).sin(),
                    Wave::Square => {
                        if v.phase < 0.5 {
                            0.6
                        } else {
                            -0.6
                        }
                    }
                    Wave::Saw => (v.phase * 2.0 - 1.0) * 0.6,
                    Wave::Tri => 1.0 - 4.0 * (v.phase - 0.5).abs(),
                    Wave::Noise => {
                        v.noise ^= v.noise << 13;
                        v.noise ^= v.noise >> 17;
                        v.noise ^= v.noise << 5;
                        (v.noise as f32 / u32::MAX as f32) * 2.0 - 1.0
                    }
                };
                let env = if v.t < v.attack { v.t / v.attack } else { (1.0 - p) * (1.0 - p) };
                let g = v.vol * env * if v.music { self.music_vol * self.duck } else { self.sfx_vol };
                let x = s * g;
                l += x * (1.0 - v.pan.max(0.0));
                r += x * (1.0 + v.pan.min(0.0));
                v.t += dt;
            }
            out[f * 2] = soft_clip(l);
            out[f * 2 + 1] = soft_clip(r);
        }
        self.voices.retain(|v| v.t < v.dur || v.delay > 0.0);
    }
}

#[inline]
fn soft_clip(x: f32) -> f32 {
    if x > 1.0 {
        1.0
    } else if x < -1.0 {
        -1.0
    } else {
        x * (1.5 - 0.5 * x * x)
    }
}

pub type SharedSynth = Arc<Mutex<Synth>>;

/// Start the platform backend. Returns false if no audio device could be opened
/// (the game still runs, silently).
pub fn start(synth: SharedSynth) -> bool {
    backend::start(synth)
}

// ------------------------------------------------------------------ Linux: ALSA via dlopen
#[cfg(target_os = "linux")]
mod backend {
    use super::*;
    use std::ffi::{c_char, c_int, c_long, c_uint, c_ulong, c_void, CString};

    type Open = unsafe extern "C" fn(*mut *mut c_void, *const c_char, c_int, c_int) -> c_int;
    type SetParams = unsafe extern "C" fn(*mut c_void, c_int, c_int, c_uint, c_uint, c_int, c_uint) -> c_int;
    type WriteI = unsafe extern "C" fn(*mut c_void, *const c_void, c_ulong) -> c_long;
    type Recover = unsafe extern "C" fn(*mut c_void, c_int, c_int) -> c_int;

    pub fn start(synth: SharedSynth) -> bool {
        unsafe {
            let mut lib = std::ptr::null_mut();
            for name in ["libasound.so.2", "libasound.so"] {
                let n = CString::new(name).unwrap();
                lib = libc::dlopen(n.as_ptr(), libc::RTLD_NOW);
                if !lib.is_null() {
                    break;
                }
            }
            if lib.is_null() {
                eprintln!("audio: libasound not found, running silent");
                return false;
            }
            let sym = |s: &str| {
                let n = CString::new(s).unwrap();
                libc::dlsym(lib, n.as_ptr())
            };
            let (o, sp, w, r) = (sym("snd_pcm_open"), sym("snd_pcm_set_params"), sym("snd_pcm_writei"), sym("snd_pcm_recover"));
            if o.is_null() || sp.is_null() || w.is_null() || r.is_null() {
                return false;
            }
            let open: Open = std::mem::transmute(o);
            let set_params: SetParams = std::mem::transmute(sp);
            let writei: WriteI = std::mem::transmute(w);
            let recover: Recover = std::mem::transmute(r);
            let mut pcm: *mut c_void = std::ptr::null_mut();
            let dev = CString::new("default").unwrap();
            if open(&mut pcm, dev.as_ptr(), 0, 0) < 0 {
                eprintln!("audio: cannot open ALSA default device");
                return false;
            }
            // S16_LE = 2, RW_INTERLEAVED = 3, 2 channels, 48 kHz, 30 ms latency
            if set_params(pcm, 2, 3, 2, SR, 1, 30_000) < 0 {
                eprintln!("audio: cannot set ALSA params");
                return false;
            }
            let pcm_addr = pcm as usize;
            std::thread::Builder::new()
                .name("audio".into())
                .spawn(move || {
                    let pcm = pcm_addr as *mut c_void;
                    let frames = 256usize;
                    let mut fbuf = vec![0f32; frames * 2];
                    let mut ibuf = vec![0i16; frames * 2];
                    loop {
                        if let Ok(mut s) = synth.lock() {
                            s.render(&mut fbuf);
                        }
                        for (d, s) in ibuf.iter_mut().zip(fbuf.iter()) {
                            *d = (s * 30000.0) as i16;
                        }
                        let n = writei(pcm, ibuf.as_ptr() as *const c_void, frames as c_ulong);
                        if n < 0 {
                            recover(pcm, n as c_int, 1);
                        }
                    }
                })
                .is_ok()
        }
    }
}

// ------------------------------------------------------------------ Windows: waveOut
#[cfg(any(windows, feature = "check_all"))]
#[allow(dead_code, non_snake_case, clippy::upper_case_acronyms)]
mod win_audio {
    use super::*;
    use std::ffi::c_void;

    #[repr(C, packed(2))]
    pub struct WAVEFORMATEX {
        pub wFormatTag: u16,
        pub nChannels: u16,
        pub nSamplesPerSec: u32,
        pub nAvgBytesPerSec: u32,
        pub nBlockAlign: u16,
        pub wBitsPerSample: u16,
        pub cbSize: u16,
    }
    #[repr(C)]
    pub struct WAVEHDR {
        pub lpData: *mut i8,
        pub dwBufferLength: u32,
        pub dwBytesRecorded: u32,
        pub dwUser: usize,
        pub dwFlags: u32,
        pub dwLoops: u32,
        pub lpNext: *mut WAVEHDR,
        pub reserved: usize,
    }
    const WHDR_DONE: u32 = 1;
    #[cfg_attr(windows, link(name = "winmm"))]
    extern "system" {
        fn waveOutOpen(phwo: *mut *mut c_void, id: u32, fmt: *const WAVEFORMATEX, cb: usize, inst: usize, flags: u32) -> u32;
        fn waveOutPrepareHeader(hwo: *mut c_void, hdr: *mut WAVEHDR, size: u32) -> u32;
        fn waveOutWrite(hwo: *mut c_void, hdr: *mut WAVEHDR, size: u32) -> u32;
    }

    pub fn start(synth: SharedSynth) -> bool {
        unsafe {
            let fmt = WAVEFORMATEX {
                wFormatTag: 1,
                nChannels: 2,
                nSamplesPerSec: SR,
                nAvgBytesPerSec: SR * 4,
                nBlockAlign: 4,
                wBitsPerSample: 16,
                cbSize: 0,
            };
            let mut hwo: *mut c_void = std::ptr::null_mut();
            if waveOutOpen(&mut hwo, 0xFFFF_FFFF, &fmt, 0, 0, 0) != 0 {
                return false;
            }
            let addr = hwo as usize;
            std::thread::Builder::new()
                .name("audio".into())
                .spawn(move || {
                    let hwo = addr as *mut c_void;
                    const N: usize = 4;
                    const FR: usize = 512;
                    let mut bufs: Vec<Vec<i16>> = (0..N).map(|_| vec![0i16; FR * 2]).collect();
                    let mut hdrs: Vec<WAVEHDR> = (0..N)
                        .map(|i| WAVEHDR {
                            lpData: bufs[i].as_mut_ptr() as *mut i8,
                            dwBufferLength: (FR * 4) as u32,
                            dwBytesRecorded: 0,
                            dwUser: 0,
                            dwFlags: WHDR_DONE,
                            dwLoops: 0,
                            lpNext: std::ptr::null_mut(),
                            reserved: 0,
                        })
                        .collect();
                    let mut fbuf = vec![0f32; FR * 2];
                    let hsize = std::mem::size_of::<WAVEHDR>() as u32;
                    for h in hdrs.iter_mut() {
                        waveOutPrepareHeader(hwo, h, hsize);
                        h.dwFlags |= WHDR_DONE;
                    }
                    loop {
                        let mut wrote = false;
                        for i in 0..N {
                            let done = std::ptr::read_volatile(&hdrs[i].dwFlags) & WHDR_DONE != 0;
                            if done {
                                if let Ok(mut s) = synth.lock() {
                                    s.render(&mut fbuf);
                                }
                                for (d, s) in bufs[i].iter_mut().zip(fbuf.iter()) {
                                    *d = (s * 30000.0) as i16;
                                }
                                hdrs[i].dwFlags &= !WHDR_DONE;
                                waveOutWrite(hwo, &mut hdrs[i], hsize);
                                wrote = true;
                            }
                        }
                        if !wrote {
                            std::thread::sleep(std::time::Duration::from_millis(2));
                        }
                    }
                })
                .is_ok()
        }
    }
}
#[cfg(windows)]
mod backend {
    pub use super::win_audio::start;
}

// ------------------------------------------------------------------ macOS: AudioQueue
#[cfg(any(target_os = "macos", feature = "check_all"))]
#[allow(dead_code, non_snake_case, non_upper_case_globals)]
mod mac_audio {
    use super::*;
    use std::ffi::c_void;

    #[repr(C)]
    pub struct AudioStreamBasicDescription {
        pub mSampleRate: f64,
        pub mFormatID: u32,
        pub mFormatFlags: u32,
        pub mBytesPerPacket: u32,
        pub mFramesPerPacket: u32,
        pub mBytesPerFrame: u32,
        pub mChannelsPerFrame: u32,
        pub mBitsPerChannel: u32,
        pub mReserved: u32,
    }
    #[repr(C)]
    pub struct AudioQueueBuffer {
        pub mAudioDataBytesCapacity: u32,
        pub mAudioData: *mut c_void,
        pub mAudioDataByteSize: u32,
        pub mUserData: *mut c_void,
        pub mPacketDescriptionCapacity: u32,
        pub mPacketDescriptions: *mut c_void,
        pub mPacketDescriptionCount: u32,
    }
    type AudioQueueRef = *mut c_void;
    type Callback = extern "C" fn(*mut c_void, AudioQueueRef, *mut AudioQueueBuffer);
    #[cfg_attr(target_os = "macos", link(name = "AudioToolbox", kind = "framework"))]
    extern "C" {
        fn AudioQueueNewOutput(
            fmt: *const AudioStreamBasicDescription,
            cb: Callback,
            user: *mut c_void,
            run_loop: *const c_void,
            mode: *const c_void,
            flags: u32,
            out: *mut AudioQueueRef,
        ) -> i32;
        fn AudioQueueAllocateBuffer(aq: AudioQueueRef, size: u32, out: *mut *mut AudioQueueBuffer) -> i32;
        fn AudioQueueEnqueueBuffer(aq: AudioQueueRef, buf: *mut AudioQueueBuffer, n: u32, desc: *const c_void) -> i32;
        fn AudioQueueStart(aq: AudioQueueRef, t: *const c_void) -> i32;
    }

    const FRAMES: u32 = 512;

    extern "C" fn callback(user: *mut c_void, aq: AudioQueueRef, buf: *mut AudioQueueBuffer) {
        unsafe {
            let synth = &*(user as *const Mutex<Synth>);
            let b = &mut *buf;
            let n = (b.mAudioDataBytesCapacity / 4) as usize;
            let out = std::slice::from_raw_parts_mut(b.mAudioData as *mut f32, n);
            if let Ok(mut s) = synth.lock() {
                s.render(out);
            } else {
                out.fill(0.0);
            }
            b.mAudioDataByteSize = b.mAudioDataBytesCapacity;
            AudioQueueEnqueueBuffer(aq, buf, 0, std::ptr::null());
        }
    }

    pub fn start(synth: SharedSynth) -> bool {
        unsafe {
            let fmt = AudioStreamBasicDescription {
                mSampleRate: SR as f64,
                mFormatID: 0x6C70_636D, // 'lpcm'
                mFormatFlags: 1 | 8,    // float | packed
                mBytesPerPacket: 8,
                mFramesPerPacket: 1,
                mBytesPerFrame: 8,
                mChannelsPerFrame: 2,
                mBitsPerChannel: 32,
                mReserved: 0,
            };
            let user = Arc::into_raw(synth) as *mut c_void;
            let mut aq: AudioQueueRef = std::ptr::null_mut();
            if AudioQueueNewOutput(&fmt, callback, user, std::ptr::null(), std::ptr::null(), 0, &mut aq) != 0 {
                return false;
            }
            for _ in 0..3 {
                let mut b: *mut AudioQueueBuffer = std::ptr::null_mut();
                if AudioQueueAllocateBuffer(aq, FRAMES * 8, &mut b) != 0 {
                    return false;
                }
                callback(user, aq, b);
            }
            AudioQueueStart(aq, std::ptr::null()) == 0
        }
    }
}
#[cfg(target_os = "macos")]
mod backend {
    pub use super::mac_audio::start;
}

#[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
mod backend {
    pub fn start(_s: super::SharedSynth) -> bool {
        false
    }
}
