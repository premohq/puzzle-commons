//! Input: keyboard layouts, gamepads (native per OS), the latch, SOCD, and the modern Quad
//! DAS/ARR/SDF interpreter that runs before the latch (outside the sim).

use minifb::{Key, KeyRepeat, Window};
use purtris_sim::common::{bits as B, Style};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Left,
    Right,
    Up,
    Down,
    Drop,
    RotA,
    RotB,
    Rot180,
    Hold,
    TNext,
    TPrev,
}
pub const ACTIONS: [Action; 11] = [
    Action::Left,
    Action::Right,
    Action::Up,
    Action::Down,
    Action::Drop,
    Action::RotA,
    Action::RotB,
    Action::Rot180,
    Action::Hold,
    Action::TNext,
    Action::TPrev,
];
impl Action {
    pub fn name(self) -> &'static str {
        match self {
            Action::Left => "Left",
            Action::Right => "Right",
            Action::Up => "Up / rotate (Panel: cursor up)",
            Action::Down => "Down / soft drop",
            Action::Drop => "Hard drop (Panel: raise)",
            Action::RotA => "Rotate CCW (Panel: swap)",
            Action::RotB => "Rotate CW (Panel: swap)",
            Action::Rot180 => "Rotate 180",
            Action::Hold => "Hold (Panel: raise)",
            Action::TNext => "Target next",
            Action::TPrev => "Target previous",
        }
    }
}

/// Which bits an action produces for a style.
pub fn action_bits(style: Style, a: Action) -> u16 {
    match (style, a) {
        (_, Action::Left) => B::L,
        (_, Action::Right) => B::R,
        // arrows alone play every style: Up rotates (Panel moves the cursor up)
        (Style::Quad, Action::Up) => B::CW,
        (Style::Panel, Action::Up) => B::U,
        (Style::Chain, Action::Up) | (Style::Capsule, Action::Up) => B::CW,
        (Style::Quad, Action::Down) => B::SD,
        (_, Action::Down) => B::D,
        (Style::Quad, Action::Drop) => B::HD,
        (Style::Panel, Action::Drop) => B::RAISE,
        (_, Action::Drop) => 0,
        (Style::Panel, Action::RotA) | (Style::Panel, Action::RotB) => B::SWAP,
        (_, Action::RotA) => B::CCW,
        (_, Action::RotB) => B::CW,
        (Style::Quad, Action::Rot180) => B::R180,
        (_, Action::Rot180) => 0,
        (Style::Quad, Action::Hold) => B::HOLD,
        (Style::Panel, Action::Hold) => B::RAISE,
        (_, Action::Hold) => 0,
        (_, Action::TNext) => B::TNEXT,
        (_, Action::TPrev) => B::TPREV,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Device {
    KbFull,
    KbLeft,
    KbRight,
    Pad(u8),
}
impl Device {
    pub fn name(self) -> String {
        match self {
            Device::KbFull => "Keyboard".into(),
            Device::KbLeft => "Keyboard left (WASD)".into(),
            Device::KbRight => "Keyboard right (arrows)".into(),
            Device::Pad(n) => format!("Gamepad {}", n + 1),
        }
    }
    pub fn layout(self) -> usize {
        match self {
            Device::KbFull => 0,
            Device::KbLeft => 1,
            Device::KbRight => 2,
            Device::Pad(_) => 3,
        }
    }
}

pub fn default_bindings() -> Vec<Vec<Vec<Key>>> {
    use Key::*;
    let full = vec![
        vec![Left],
        vec![Right],
        vec![Up],
        vec![Down],
        vec![Space],
        vec![Z],
        vec![X],
        vec![A],
        vec![C, LeftShift],
        vec![Tab],
        vec![Q],
    ];
    let left = vec![vec![A], vec![D], vec![W], vec![S], vec![LeftShift], vec![F], vec![G], vec![R], vec![H], vec![T], vec![E]];
    let right = vec![
        vec![Left],
        vec![Right],
        vec![Up],
        vec![Down],
        vec![RightShift],
        vec![Comma],
        vec![Period],
        vec![L],
        vec![Slash],
        vec![Semicolon],
        vec![Apostrophe],
    ];
    vec![full, left, right]
}

// ------------------------------------------------------------------ gamepads

pub mod pad {
    pub const UP: u32 = 1;
    pub const DOWN: u32 = 2;
    pub const LEFT: u32 = 4;
    pub const RIGHT: u32 = 8;
    pub const A: u32 = 16;
    pub const B: u32 = 32;
    pub const X: u32 = 64;
    pub const Y: u32 = 128;
    pub const LB: u32 = 256;
    pub const RB: u32 = 512;
    pub const LT: u32 = 1024;
    pub const RT: u32 = 2048;
    pub const START: u32 = 4096;
    pub const BACK: u32 = 8192;
}

pub fn pad_action(a: Action) -> u32 {
    match a {
        Action::Left => pad::LEFT,
        Action::Right => pad::RIGHT,
        Action::Up => pad::UP,
        Action::Down => pad::DOWN,
        Action::Drop => pad::RB,
        Action::RotA => pad::A,
        Action::RotB => pad::B,
        Action::Rot180 => pad::Y,
        Action::Hold => pad::X | pad::LB,
        Action::TNext => pad::RT,
        Action::TPrev => pad::LT,
    }
}

#[cfg(target_os = "linux")]
mod pads {
    //! Deep controller support on Linux, written from scratch: evdev devices are discovered
    //! and hot-plugged, identified by their SDL GUID, and mapped through the embedded SDL
    //! GameControllerDB (zlib license). Devices not in the database use the kernel's standard
    //! gamepad layout, so Xbox, PlayStation, Switch, 8BitDo, arcade sticks and generic HID
    //! pads all work without drivers or config.
    use super::pad::*;
    use std::collections::HashMap;
    use std::fs::File;
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;
    use std::os::unix::io::AsRawFd;

    const DB: &str = include_str!("../../../assets/input/gamecontrollerdb.txt");
    const EV_KEY: u16 = 1;
    const EV_ABS: u16 = 3;
    const BTN_JOYSTICK: usize = 0x120;
    const KEY_MAX: usize = 0x2ff;
    const ABS_MAX: usize = 0x3f;
    const ABS_HAT0X: usize = 0x10;
    const ABS_HAT3Y: usize = 0x17;

    fn ioc_read(nr: u64, len: u64) -> u64 {
        (2 << 30) | (len << 16) | ((b'E' as u64) << 8) | nr
    }
    fn ioctl_buf(fd: i32, req: u64, buf: &mut [u8]) -> bool {
        unsafe { libc::ioctl(fd, req as _, buf.as_mut_ptr()) >= 0 }
    }
    fn bit(bits: &[u8], i: usize) -> bool {
        bits.get(i / 8).map(|b| b & (1 << (i % 8)) != 0).unwrap_or(false)
    }

    #[derive(Clone, Copy, Debug)]
    enum Src {
        Btn(usize),
        Axis { idx: usize, half: i8, invert: bool },
        Hat { idx: usize, mask: u8 },
    }

    struct Dev {
        path: String,
        file: File,
        pub name: String,
        key_index: HashMap<u16, usize>,
        abs_index: HashMap<u16, usize>,
        hat_index: HashMap<u16, (usize, bool)>,
        abs_range: HashMap<u16, (i32, i32)>,
        buttons: Vec<bool>,
        axes: Vec<f32>,
        hats: Vec<(i32, i32)>,
        map: Option<HashMap<String, Src>>,
        raw_keys: HashMap<u16, bool>,
        raw_abs: HashMap<u16, f32>,
    }

    fn parse_src(v: &str) -> Option<Src> {
        let (half, rest) = if let Some(r) = v.strip_prefix('+') {
            (1, r)
        } else if let Some(r) = v.strip_prefix('-') {
            (-1, r)
        } else {
            (0, v)
        };
        let (rest, invert) = match rest.strip_suffix('~') {
            Some(r) => (r, true),
            None => (rest, false),
        };
        if let Some(n) = rest.strip_prefix('b') {
            return n.parse().ok().map(Src::Btn);
        }
        if let Some(n) = rest.strip_prefix('a') {
            return n.parse().ok().map(|idx| Src::Axis { idx, half, invert });
        }
        if let Some(n) = rest.strip_prefix('h') {
            let (i, m) = n.split_once('.')?;
            return Some(Src::Hat { idx: i.parse().ok()?, mask: m.parse().ok()? });
        }
        None
    }

    fn lookup(guid: &str, guid_nover: &str) -> Option<HashMap<String, Src>> {
        let mut fallback = None;
        for line in DB.lines() {
            if line.starts_with('#') || !line.contains("platform:Linux") {
                continue;
            }
            let id = &line[..line.find(',').unwrap_or(0)];
            let exact = id.eq_ignore_ascii_case(guid);
            let loose = id.len() == 32 && guid_nover.len() == 32 && id[..24].eq_ignore_ascii_case(&guid_nover[..24]);
            if !exact && !loose {
                continue;
            }
            let mut m = HashMap::new();
            for part in line.split(',').skip(2) {
                if let Some((k, v)) = part.split_once(':') {
                    if let Some(src) = parse_src(v) {
                        m.insert(k.to_string(), src);
                    }
                }
            }
            if exact {
                return Some(m);
            }
            fallback = Some(m);
        }
        fallback
    }

    fn open_dev(path: &str) -> Option<Dev> {
        let file = std::fs::OpenOptions::new().read(true).custom_flags(libc::O_NONBLOCK).open(path).ok()?;
        let fd = file.as_raw_fd();
        let mut keys = [0u8; KEY_MAX / 8 + 1];
        let mut abs = [0u8; ABS_MAX / 8 + 1];
        if !ioctl_buf(fd, ioc_read(0x20 + EV_KEY as u64, keys.len() as u64), &mut keys) {
            return None;
        }
        ioctl_buf(fd, ioc_read(0x20 + EV_ABS as u64, abs.len() as u64), &mut abs);
        // a gamepad or joystick: has joystick/gamepad buttons and either sticks or a hat
        let has_pad_btn = (BTN_JOYSTICK..0x140).any(|i| bit(&keys, i)) || (0x220..0x224).any(|i| bit(&keys, i));
        let has_axes = bit(&abs, 0) || bit(&abs, ABS_HAT0X);
        if !has_pad_btn || !has_axes {
            return None;
        }
        let mut id = [0u8; 8];
        ioctl_buf(fd, ioc_read(0x02, 8), &mut id);
        let w = |i: usize| u16::from_le_bytes([id[i], id[i + 1]]);
        let (bus, vendor, product, version) = (w(0), w(2), w(4), w(6));
        let mut nm = [0u8; 128];
        ioctl_buf(fd, ioc_read(0x06, 128), &mut nm);
        let name = String::from_utf8_lossy(&nm[..nm.iter().position(|&c| c == 0).unwrap_or(0)]).to_string();
        let hex = |v: u16| format!("{:02x}{:02x}", v & 0xff, v >> 8);
        let guid = format!("{}0000{}0000{}0000{}0000", hex(bus), hex(vendor), hex(product), hex(version));
        let guid0 = format!("{}0000{}0000{}0000{}0000", hex(bus), hex(vendor), hex(product), hex(0));
        // SDL's index order: buttons from BTN_JOYSTICK upward, then the rest; axes skip hats
        let mut key_index = HashMap::new();
        let mut n = 0;
        for i in (BTN_JOYSTICK..KEY_MAX).chain(0..BTN_JOYSTICK) {
            if bit(&keys, i) {
                key_index.insert(i as u16, n);
                n += 1;
            }
        }
        let mut abs_index = HashMap::new();
        let mut abs_range = HashMap::new();
        let mut hat_index = HashMap::new();
        let mut na = 0;
        let mut nh = 0;
        let mut i = 0;
        while i < ABS_MAX {
            if i == ABS_HAT0X {
                let mut h = ABS_HAT0X;
                while h <= ABS_HAT3Y {
                    if bit(&abs, h) || bit(&abs, h + 1) {
                        hat_index.insert(h as u16, (nh, true));
                        hat_index.insert((h + 1) as u16, (nh, false));
                        nh += 1;
                    }
                    h += 2;
                }
                i = ABS_HAT3Y + 1;
                continue;
            }
            if bit(&abs, i) {
                abs_index.insert(i as u16, na);
                na += 1;
                let mut info = [0u8; 24];
                if ioctl_buf(fd, ioc_read(0x40 + i as u64, 24), &mut info) {
                    let mn = i32::from_le_bytes([info[4], info[5], info[6], info[7]]);
                    let mx = i32::from_le_bytes([info[8], info[9], info[10], info[11]]);
                    abs_range.insert(i as u16, (mn, mx.max(mn + 1)));
                }
            }
            i += 1;
        }
        let map = lookup(&guid, &guid0);
        Some(Dev {
            path: path.to_string(),
            file,
            name,
            key_index,
            abs_index,
            hat_index,
            abs_range,
            buttons: vec![false; n],
            axes: vec![0.0; na],
            hats: vec![(0, 0); nh],
            map,
            raw_keys: HashMap::new(),
            raw_abs: HashMap::new(),
        })
    }

    impl Dev {
        fn pump(&mut self) -> bool {
            let mut buf = [0u8; 24 * 64];
            loop {
                match self.file.read(&mut buf) {
                    Ok(n) if n >= 24 => {
                        for ev in buf[..n - n % 24].chunks(24) {
                            let typ = u16::from_le_bytes([ev[16], ev[17]]);
                            let code = u16::from_le_bytes([ev[18], ev[19]]);
                            let value = i32::from_le_bytes([ev[20], ev[21], ev[22], ev[23]]);
                            if typ == EV_KEY {
                                if let Some(&k) = self.key_index.get(&code) {
                                    self.buttons[k] = value != 0;
                                }
                                self.raw_keys.insert(code, value != 0);
                            } else if typ == EV_ABS {
                                if let Some(&(h, is_x)) = self.hat_index.get(&code) {
                                    let v = value.signum();
                                    if is_x {
                                        self.hats[h].0 = v;
                                    } else {
                                        self.hats[h].1 = v;
                                    }
                                    self.raw_abs.insert(code, v as f32);
                                } else if let Some(&a) = self.abs_index.get(&code) {
                                    let (mn, mx) = self.abs_range.get(&code).copied().unwrap_or((-32768, 32767));
                                    let s = ((value - mn) as f32 / (mx - mn) as f32) * 2.0 - 1.0;
                                    self.axes[a] = s;
                                    self.raw_abs.insert(code, s);
                                }
                            }
                        }
                    }
                    Ok(_) => return true,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return true,
                    Err(_) => return false,
                }
            }
        }

        fn eval(&self, src: Src, trigger: bool) -> bool {
            match src {
                Src::Btn(i) => self.buttons.get(i).copied().unwrap_or(false),
                Src::Axis { idx, half, invert } => {
                    let mut v = self.axes.get(idx).copied().unwrap_or(0.0);
                    if invert {
                        v = -v;
                    }
                    match half {
                        1 => v > 0.5,
                        -1 => v < -0.5,
                        _ if trigger => v > 0.1,
                        _ => v > 0.5,
                    }
                }
                Src::Hat { idx, mask } => {
                    let (x, y) = self.hats.get(idx).copied().unwrap_or((0, 0));
                    let bits = (if y < 0 { 1 } else { 0 }) | (if x > 0 { 2 } else { 0 }) | (if y > 0 { 4 } else { 0 }) | (if x < 0 { 8 } else { 0 });
                    bits & mask != 0
                }
            }
        }

        fn state(&self, analog: bool) -> u32 {
            let mut s = 0;
            if let Some(m) = &self.map {
                let table = [
                    ("a", A, false),
                    ("b", B, false),
                    ("x", X, false),
                    ("y", Y, false),
                    ("leftshoulder", LB, false),
                    ("rightshoulder", RB, false),
                    ("lefttrigger", LT, true),
                    ("righttrigger", RT, true),
                    ("start", START, false),
                    ("back", BACK, false),
                    ("dpup", UP, false),
                    ("dpdown", DOWN, false),
                    ("dpleft", LEFT, false),
                    ("dpright", RIGHT, false),
                ];
                for (k, bitv, trig) in table {
                    if let Some(&src) = m.get(k) {
                        if self.eval(src, trig) {
                            s |= bitv;
                        }
                    }
                }
                if analog {
                    let ax = |k: &str| match m.get(k) {
                        Some(Src::Axis { idx, invert, .. }) => {
                            let v = self.axes.get(*idx).copied().unwrap_or(0.0);
                            if *invert {
                                -v
                            } else {
                                v
                            }
                        }
                        _ => 0.0,
                    };
                    let (x, y) = (ax("leftx"), ax("lefty"));
                    s |= stick(x, y);
                }
                return s;
            }
            // kernel standard gamepad layout (and old joystick buttons) when not in the DB
            let k = |c: u16| self.raw_keys.get(&c).copied().unwrap_or(false);
            let table: [(u16, u32); 24] = [
                (0x130, A), (0x131, B), (0x133, X), (0x134, Y), (0x136, LB), (0x137, RB), (0x138, LT), (0x139, RT),
                (0x13a, BACK), (0x13b, START), (0x220, UP), (0x221, DOWN), (0x222, LEFT), (0x223, RIGHT),
                (0x120, A), (0x121, B), (0x122, X), (0x123, Y), (0x124, LB), (0x125, RB), (0x126, LT), (0x127, RT), (0x128, BACK), (0x129, START),
            ];
            for (c, bitv) in table {
                if k(c) {
                    s |= bitv;
                }
            }
            let a = |c: u16| self.raw_abs.get(&c).copied().unwrap_or(0.0);
            let (hx, hy) = (a(0x10), a(0x11));
            if hx < -0.5 {
                s |= LEFT;
            }
            if hx > 0.5 {
                s |= RIGHT;
            }
            if hy < -0.5 {
                s |= UP;
            }
            if hy > 0.5 {
                s |= DOWN;
            }
            if self.abs_range.contains_key(&0x02) && a(0x02) > 0.1 && self.abs_range.get(&0x02).map(|r| r.0 >= 0).unwrap_or(false) {
                s |= LT;
            }
            if self.abs_range.contains_key(&0x05) && a(0x05) > 0.1 && self.abs_range.get(&0x05).map(|r| r.0 >= 0).unwrap_or(false) {
                s |= RT;
            }
            if analog {
                s |= stick(a(0x00), a(0x01));
            }
            s
        }
    }

    fn stick(x: f32, y: f32) -> u32 {
        let mut s = 0;
        if x < -0.6 {
            s |= LEFT;
        }
        if x > 0.6 {
            s |= RIGHT;
        }
        if y < -0.6 {
            s |= UP;
        }
        if y > 0.6 {
            s |= DOWN;
        }
        s
    }

    pub struct Pads {
        devs: Vec<Dev>,
        scan_t: u32,
        tried: HashMap<String, u32>,
    }
    impl Pads {
        pub fn new() -> Self {
            let mut p = Pads { devs: Vec::new(), scan_t: 0, tried: HashMap::new() };
            p.scan();
            p
        }
        fn scan(&mut self) {
            let Ok(rd) = std::fs::read_dir("/dev/input") else { return };
            let mut paths: Vec<String> = rd.filter_map(|e| e.ok()).map(|e| e.path().to_string_lossy().to_string()).filter(|p| p.contains("/event")).collect();
            paths.sort();
            for p in paths {
                if self.devs.iter().any(|d| d.path == p) {
                    continue;
                }
                // do not hammer devices that are not pads (keyboards, mice) every scan
                let t = self.tried.entry(p.clone()).or_insert(0);
                if *t > 0 && self.scan_t < *t + 600 {
                    continue;
                }
                *t = self.scan_t.max(1);
                if let Some(d) = open_dev(&p) {
                    eprintln!("gamepad: {} ({}){}", d.name, p, if d.map.is_some() { " [SDL mapping]" } else { " [standard layout]" });
                    self.devs.push(d);
                }
            }
        }
        pub fn poll(&mut self, analog: bool) -> [u32; 4] {
            self.scan_t += 1;
            if self.scan_t % 90 == 0 {
                self.scan();
            }
            self.devs.retain_mut(|d| d.pump());
            let mut out = [0u32; 4];
            for (i, d) in self.devs.iter().take(4).enumerate() {
                out[i] = d.state(analog);
            }
            out
        }
        pub fn names(&self) -> Vec<String> {
            self.devs.iter().map(|d| d.name.clone()).collect()
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod pads {
    //! Windows and macOS: gilrs (Windows.Gaming.Input / IOKit) with the SDL controller database.
    use gilrs::{Axis, Button, Gilrs};
    pub struct Pads {
        g: Option<Gilrs>,
    }
    impl Pads {
        pub fn new() -> Self {
            Pads { g: Gilrs::new().ok() }
        }
        pub fn poll(&mut self, analog: bool) -> [u32; 4] {
            use super::pad::*;
            let mut out = [0u32; 4];
            let Some(g) = self.g.as_mut() else { return out };
            while g.next_event().is_some() {}
            for (i, (_id, gp)) in g.gamepads().take(4).enumerate() {
                let mut s = 0;
                let map = [
                    (Button::DPadUp, UP),
                    (Button::DPadDown, DOWN),
                    (Button::DPadLeft, LEFT),
                    (Button::DPadRight, RIGHT),
                    (Button::South, A),
                    (Button::East, B),
                    (Button::West, X),
                    (Button::North, Y),
                    (Button::LeftTrigger, LB),
                    (Button::RightTrigger, RB),
                    (Button::LeftTrigger2, LT),
                    (Button::RightTrigger2, RT),
                    (Button::Start, START),
                    (Button::Select, BACK),
                ];
                for (b, v) in map {
                    if gp.is_pressed(b) {
                        s |= v;
                    }
                }
                if analog {
                    let x = gp.value(Axis::LeftStickX);
                    let y = gp.value(Axis::LeftStickY);
                    if x < -0.6 {
                        s |= LEFT;
                    }
                    if x > 0.6 {
                        s |= RIGHT;
                    }
                    if y > 0.6 {
                        s |= UP;
                    }
                    if y < -0.6 {
                        s |= DOWN;
                    }
                }
                out[i] = s;
            }
            out
        }
        pub fn names(&self) -> Vec<String> {
            self.g.as_ref().map(|g| g.gamepads().map(|(_, p)| p.name().to_string()).collect()).unwrap_or_default()
        }
    }
}

// ------------------------------------------------------------------ the input hub

#[derive(Clone, Copy, Default)]
pub struct Nav {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub ok: bool,
    pub back: bool,
    pub pause: bool,
    pub click: bool,
    pub mouse: Option<(i32, i32)>,
    /// the pointer moved this frame: only then may hovering change a selection, so a mouse
    /// resting over the window never fights the arrow keys
    pub moved: bool,
    pub wheel: i32,
}

pub struct Input {
    pub bindings: Vec<Vec<Vec<Key>>>,
    pads: pads::Pads,
    pub pad_now: [u32; 4],
    pad_prev: [u32; 4],
    pad_latch: [u32; 4],
    held: Vec<Key>,
    latched: Vec<Key>,
    pressed_frame: Vec<Key>,
    pub analog: bool,
    pub socd_neutral: bool,
    /// seconds each menu direction has been held (key repeat is time based, not per frame)
    nav_held: [f32; 4],
    pub typed: std::sync::Arc<std::sync::Mutex<Vec<char>>>,
    mouse_prev: bool,
    mouse_last: Option<(i32, i32)>,
    pub last_device: Option<Device>,
    /// (offset x, offset y, scale) from window pixels to the 1280x720 canvas
    pub mouse_map: (f32, f32, f32),
}

struct TypeCb(std::sync::Arc<std::sync::Mutex<Vec<char>>>);
impl minifb::InputCallback for TypeCb {
    fn add_char(&mut self, uni_char: u32) {
        if let Some(c) = char::from_u32(uni_char) {
            if let Ok(mut v) = self.0.lock() {
                v.push(c);
            }
        }
    }
}

impl Input {
    pub fn new(win: Option<&mut Window>) -> Self {
        let typed = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        if let Some(win) = win {
            win.set_input_callback(Box::new(TypeCb(typed.clone())));
        }
        Input {
            bindings: default_bindings(),
            pads: pads::Pads::new(),
            pad_now: [0; 4],
            pad_prev: [0; 4],
            pad_latch: [0; 4],
            held: Vec::new(),
            latched: Vec::new(),
            pressed_frame: Vec::new(),
            analog: false,
            socd_neutral: true,
            nav_held: [0.0; 4],
            typed,
            mouse_prev: false,
            mouse_last: None,
            last_device: None,
            mouse_map: (0.0, 0.0, 1.0),
        }
    }

    /// Hook typed characters to a (new) window.
    pub fn attach(&mut self, win: &mut Window) {
        win.set_input_callback(Box::new(TypeCb(self.typed.clone())));
    }

    pub fn pad_names(&self) -> Vec<String> {
        self.pads.names()
    }

    /// Poll once per rendered frame.
    pub fn poll(&mut self, win: Option<&Window>) {
        let Some(win) = win else {
            self.held.clear();
            self.pressed_frame.clear();
            return;
        };
        self.held = win.get_keys();
        self.pressed_frame = win.get_keys_pressed(KeyRepeat::No);
        for k in &self.pressed_frame {
            if !self.latched.contains(k) {
                self.latched.push(*k);
            }
        }
        self.pad_prev = self.pad_now;
        self.pad_now = self.pads.poll(self.analog);
        for i in 0..4 {
            self.pad_latch[i] |= self.pad_now[i];
        }
        if !self.pressed_frame.is_empty() {
            self.last_device = Some(Device::KbFull);
        }
        for i in 0..4 {
            if self.pad_now[i] & !self.pad_prev[i] != 0 {
                self.last_device = Some(Device::Pad(i as u8));
            }
        }
    }

    /// Add synthetic keys (scripted tests): pressed this frame, and held.
    pub fn inject(&mut self, pressed: &[Key], held: &[Key]) {
        for k in pressed {
            if !self.pressed_frame.contains(k) {
                self.pressed_frame.push(*k);
            }
            if !self.latched.contains(k) {
                self.latched.push(*k);
            }
        }
        for k in held {
            if !self.held.contains(k) {
                self.held.push(*k);
            }
        }
    }

    pub fn key_pressed(&self, k: Key) -> bool {
        self.pressed_frame.contains(&k)
    }
    pub fn key_held(&self, k: Key) -> bool {
        self.held.contains(&k)
    }
    pub fn any_key_pressed(&self) -> Option<Key> {
        self.pressed_frame.first().copied()
    }
    pub fn take_typed(&mut self) -> Vec<char> {
        self.typed.lock().map(|mut v| std::mem::take(&mut *v)).unwrap_or_default()
    }
    pub fn pad_pressed(&self, i: usize, mask: u32) -> bool {
        self.pad_now[i] & !self.pad_prev[i] & mask != 0
    }

    /// Raw action levels for a device: held, or pressed since the last tick (latch).
    fn action_down(&self, dev: Device, a: Action) -> bool {
        match dev {
            Device::Pad(n) => self.pad_latch[n as usize] & pad_action(a) != 0,
            _ => {
                let keys = &self.bindings[dev.layout()][ACTIONS.iter().position(|&x| x == a).unwrap()];
                keys.iter().any(|k| self.held.contains(k) || self.latched.contains(k))
            }
        }
    }

    /// Latched bitfield for one seat (before the Quad interpreter). SOCD neutral applied.
    pub fn sample(&self, dev: Device, style: Style) -> u16 {
        let mut bits = 0u16;
        for &a in ACTIONS.iter() {
            if self.action_down(dev, a) {
                // console convention on gamepads: d-pad up hard drops in Quad
                bits |= if matches!(dev, Device::Pad(_)) && style == Style::Quad && a == Action::Up { B::HD } else { action_bits(style, a) };
            }
        }
        if self.socd_neutral {
            if bits & B::L != 0 && bits & B::R != 0 {
                bits &= !(B::L | B::R);
            }
            if bits & B::U != 0 && bits & B::D != 0 {
                bits &= !(B::U | B::D);
            }
        }
        bits
    }

    /// Call after every sim tick has sampled.
    pub fn clear_latch(&mut self) {
        self.latched.clear();
        self.pad_latch = self.pad_now;
    }

    /// Menu presses for one device (each seat moves its own cursor on the select screen):
    /// (left, right, up, down, ok, back).
    pub fn dev_menu(&self, dev: Device) -> (bool, bool, bool, bool, bool, bool) {
        match dev {
            Device::Pad(n) => {
                let n = n as usize;
                (self.pad_pressed(n, pad::LEFT), self.pad_pressed(n, pad::RIGHT), self.pad_pressed(n, pad::UP), self.pad_pressed(n, pad::DOWN), self.pad_pressed(n, pad::A | pad::START), self.pad_pressed(n, pad::B))
            }
            _ => {
                let lay = dev.layout();
                let hit = |a: Action| {
                    let i = ACTIONS.iter().position(|&x| x == a).unwrap();
                    self.bindings[lay][i].iter().any(|k| self.pressed_frame.contains(k))
                };
                let full = dev == Device::KbFull;
                (
                    hit(Action::Left),
                    hit(Action::Right),
                    hit(Action::Up),
                    hit(Action::Down),
                    hit(Action::RotA) || (full && (self.key_pressed(Key::Enter) || self.key_pressed(Key::Space))),
                    hit(Action::RotB) || (full && self.key_pressed(Key::Escape)),
                )
            }
        }
    }

    pub fn pause_pressed(&self) -> bool {
        self.key_pressed(Key::Escape) || self.key_pressed(Key::P) || (0..4).any(|i| self.pad_pressed(i, pad::START))
    }

    /// Menu navigation with key repeat.
    pub fn nav(&mut self, win: Option<&Window>, dt: f32) -> Nav {
        let mut n = Nav::default();
        let Some(win) = win else { return n };
        let any_pad = |m: u32| (0..4).any(|i| self.pad_now[i] & m != 0);
        let dirs = [
            self.key_held(Key::Up) || self.key_held(Key::W) || any_pad(pad::UP),
            self.key_held(Key::Down) || self.key_held(Key::S) || any_pad(pad::DOWN),
            self.key_held(Key::Left) || self.key_held(Key::A) || any_pad(pad::LEFT),
            self.key_held(Key::Right) || self.key_held(Key::D) || any_pad(pad::RIGHT),
        ];
        // first press fires at once; holding repeats after 0.32 s, then every 0.07 s
        const DELAY: f32 = 0.32;
        const EVERY: f32 = 0.07;
        let mut fire = [false; 4];
        for i in 0..4 {
            if dirs[i] {
                let before = self.nav_held[i];
                let after = before + dt.clamp(0.001, 0.1);
                self.nav_held[i] = after;
                let step = |t: f32| if t < DELAY { -1 } else { ((t - DELAY) / EVERY) as i32 };
                fire[i] = before == 0.0 || step(after) > step(before);
            } else {
                self.nav_held[i] = 0.0;
            }
        }
        n.up = fire[0];
        n.down = fire[1];
        n.left = fire[2];
        n.right = fire[3];
        n.ok = self.key_pressed(Key::Enter)
            || self.key_pressed(Key::Space)
            || self.key_pressed(Key::Z)
            || self.key_pressed(Key::NumPadEnter)
            || (0..4).any(|i| self.pad_pressed(i, pad::A | pad::START));
        n.back = self.key_pressed(Key::Escape) || self.key_pressed(Key::Backspace) || self.key_pressed(Key::X) || (0..4).any(|i| self.pad_pressed(i, pad::B));
        n.pause = self.pause_pressed();
        if let Some((mx, my)) = win.get_mouse_pos(minifb::MouseMode::Discard) {
            let (ox, oy, sc) = self.mouse_map;
            n.mouse = Some((((mx - ox) / sc) as i32, ((my - oy) / sc) as i32));
        }
        // the first position seen is not a move (the pointer may just be resting there)
        n.moved = self.mouse_last.is_some() && n.mouse.is_some() && n.mouse != self.mouse_last;
        if n.mouse.is_some() {
            self.mouse_last = n.mouse;
        }
        let down = win.get_mouse_down(minifb::MouseButton::Left);
        n.click = down && !self.mouse_prev;
        self.mouse_prev = down;
        if let Some((_, wy)) = win.get_scroll_wheel() {
            n.wheel = if wy > 0.0 { -1 } else if wy < 0.0 { 1 } else { 0 };
        }
        n
    }
}

/// Modern Quad handling interpreter (client side). The sim never sees these numbers.
#[derive(Clone, Copy, Debug)]
pub struct Handling {
    pub das: u32,
    pub arr: u32,
    /// ticks per cell while soft dropping; 0 = instant (sonic drop)
    pub sdf: u32,
}
impl Default for Handling {
    fn default() -> Self {
        Handling { das: 10, arr: 2, sdf: 1 }
    }
}

#[derive(Clone, Copy, Default, Debug)]
pub struct DasState {
    dir: i32,
    charge: u32,
    arr_t: u32,
    sd_t: u32,
    prev_hd: bool,
}

pub fn interpret_quad(raw: u16, st: &mut DasState, h: &Handling) -> u16 {
    let mut out = raw & !(B::L | B::R | B::SD | B::HD | B::FAST);
    let dir = if raw & B::L != 0 {
        -1
    } else if raw & B::R != 0 {
        1
    } else {
        0
    };
    let dbit = if dir < 0 { B::L } else { B::R };
    if dir == 0 {
        st.dir = 0;
        st.charge = 0;
        st.arr_t = 0;
    } else if dir != st.dir {
        st.dir = dir;
        st.charge = 0;
        st.arr_t = 0;
        out |= dbit;
    } else {
        st.charge += 1;
        if st.charge >= h.das {
            if h.arr == 0 {
                out |= dbit | B::FAST;
            } else {
                st.arr_t += 1;
                if st.arr_t >= h.arr {
                    st.arr_t = 0;
                    out |= dbit;
                }
            }
        }
    }
    if raw & B::SD != 0 {
        if h.sdf == 0 {
            out |= B::SD | B::FAST;
        } else {
            st.sd_t += 1;
            if st.sd_t >= h.sdf {
                st.sd_t = 0;
                out |= B::SD;
            }
        }
    } else {
        st.sd_t = h.sdf.saturating_sub(1);
    }
    let hd = raw & B::HD != 0;
    if hd && !st.prev_hd {
        out |= B::HD;
    }
    st.prev_hd = hd;
    out
}

/// All keys a player may bind, for saving and loading.
pub fn key_from_name(s: &str) -> Option<Key> {
    use Key::*;
    let all = [
        Key0, Key1, Key2, Key3, Key4, Key5, Key6, Key7, Key8, Key9, A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z, F1, F2, F3, F4,
        F5, F6, F7, F8, F9, F10, F11, F12, Down, Left, Right, Up, Apostrophe, Backquote, Backslash, Comma, Equal, LeftBracket, Minus, Period, RightBracket,
        Semicolon, Slash, Backspace, Delete, End, Enter, Home, Insert, PageDown, PageUp, Space, Tab, LeftShift, RightShift, LeftCtrl, RightCtrl, LeftAlt,
        RightAlt, NumPad0, NumPad1, NumPad2, NumPad3, NumPad4, NumPad5, NumPad6, NumPad7, NumPad8, NumPad9, NumPadDot, NumPadSlash, NumPadAsterisk,
        NumPadMinus, NumPadPlus, NumPadEnter, Escape,
    ];
    all.iter().copied().find(|k| format!("{:?}", k) == s)
}
pub fn key_name(k: Key) -> String {
    let s = format!("{:?}", k);
    s.strip_prefix("Key").map(|x| x.to_string()).unwrap_or(s)
}
