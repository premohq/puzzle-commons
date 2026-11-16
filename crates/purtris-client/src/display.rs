//! The platform display layer: the primary monitor's size and refresh rate, and true fullscreen.
//! Linux talks to X11 (XWayland too) through libX11/libXrandr loaded at run time, so nothing
//! extra is needed to build; Windows uses user32/gdi32; macOS uses CoreGraphics.

/// Primary monitor: (x, y, width, height) in pixels, and refresh rate in Hz (0 when unknown).
#[derive(Clone, Copy, Debug)]
pub struct Monitor {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub hz: f32,
}

impl Default for Monitor {
    fn default() -> Self {
        Monitor { x: 0, y: 0, w: 1920, h: 1080, hz: 60.0 }
    }
}

/// A display mode the monitor offers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mode {
    pub w: i32,
    pub h: i32,
    pub hz: f32,
}

/// Every mode the primary monitor offers (largest first). Empty when the system will not say.
pub fn modes() -> Vec<Mode> {
    #[cfg(target_os = "linux")]
    let mut v = x11::modes().unwrap_or_default();
    #[cfg(windows)]
    let mut v = win::modes();
    #[cfg(target_os = "macos")]
    let mut v = mac::modes();
    #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
    let mut v: Vec<Mode> = Vec::new();
    v.retain(|m| m.w >= 640 && m.h >= 360);
    v.sort_by(|a, b| (b.w, b.h).cmp(&(a.w, a.h)).then(b.hz.partial_cmp(&a.hz).unwrap_or(std::cmp::Ordering::Equal)));
    v.dedup_by(|a, b| a.w == b.w && a.h == b.h && (a.hz - b.hz).abs() < 0.5);
    v
}

/// Call once before any window exists (Windows: report real pixels, not DPI-scaled ones).
pub fn init() {
    #[cfg(windows)]
    win::init();
}

pub fn primary() -> Monitor {
    #[cfg(target_os = "linux")]
    {
        x11::primary().unwrap_or_default()
    }
    #[cfg(windows)]
    {
        win::primary()
    }
    #[cfg(target_os = "macos")]
    {
        mac::primary()
    }
    #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
    {
        Monitor::default()
    }
}

/// Ask the window manager to make this window fullscreen (Linux; elsewhere a borderless,
/// monitor-sized window already is).
pub fn make_fullscreen(win: &minifb::Window) {
    #[cfg(target_os = "linux")]
    x11::fullscreen(win);
    #[cfg(not(target_os = "linux"))]
    let _ = win;
}

#[cfg(target_os = "linux")]
mod x11 {
    use super::Monitor;
    use std::ffi::{c_char, c_int, c_long, c_ulong, c_void, CStr};

    type XOpenDisplay = unsafe extern "C" fn(*const c_char) -> *mut c_void;
    type XCloseDisplay = unsafe extern "C" fn(*mut c_void) -> c_int;
    type XDefaultRootWindow = unsafe extern "C" fn(*mut c_void) -> c_ulong;
    type XDefaultScreen = unsafe extern "C" fn(*mut c_void) -> c_int;
    type XDisplayDim = unsafe extern "C" fn(*mut c_void, c_int) -> c_int;
    type XInternAtom = unsafe extern "C" fn(*mut c_void, *const c_char, c_int) -> c_ulong;
    type XSendEvent = unsafe extern "C" fn(*mut c_void, c_ulong, c_int, c_long, *mut c_long) -> c_int;
    type XFlush = unsafe extern "C" fn(*mut c_void) -> c_int;
    type XRRGetMonitors = unsafe extern "C" fn(*mut c_void, c_ulong, c_int, *mut c_int) -> *mut MonitorInfo;
    type XRRFreeMonitors = unsafe extern "C" fn(*mut MonitorInfo);
    type XRRGetScreenResourcesCurrent = unsafe extern "C" fn(*mut c_void, c_ulong) -> *mut ScreenResources;
    type XRRFreeScreenResources = unsafe extern "C" fn(*mut ScreenResources);
    type XRRGetCrtcInfo = unsafe extern "C" fn(*mut c_void, *mut ScreenResources, c_ulong) -> *mut CrtcInfo;
    type XRRFreeCrtcInfo = unsafe extern "C" fn(*mut CrtcInfo);

    /// XRRMonitorInfo (Xrandr.h)
    #[repr(C)]
    pub struct MonitorInfo {
        name: c_ulong,
        primary: c_int,
        automatic: c_int,
        noutput: c_int,
        x: c_int,
        y: c_int,
        width: c_int,
        height: c_int,
        mwidth: c_int,
        mheight: c_int,
        outputs: *mut c_ulong,
    }
    /// XRRScreenResources
    #[repr(C)]
    pub struct ScreenResources {
        timestamp: c_ulong,
        config_timestamp: c_ulong,
        ncrtc: c_int,
        crtcs: *mut c_ulong,
        noutput: c_int,
        outputs: *mut c_ulong,
        nmode: c_int,
        modes: *mut ModeInfo,
    }
    /// XRRModeInfo
    #[repr(C)]
    struct ModeInfo {
        id: c_ulong,
        width: u32,
        height: u32,
        dot_clock: c_ulong,
        h_sync_start: u32,
        h_sync_end: u32,
        h_total: u32,
        h_skew: u32,
        v_sync_start: u32,
        v_sync_end: u32,
        v_total: u32,
        name: *mut c_char,
        name_length: u32,
        mode_flags: c_ulong,
    }
    /// XRRCrtcInfo
    #[repr(C)]
    pub struct CrtcInfo {
        timestamp: c_ulong,
        x: c_int,
        y: c_int,
        width: u32,
        height: u32,
        mode: c_ulong,
        rotation: u16,
        noutput: c_int,
        outputs: *mut c_ulong,
        rotations: u16,
        npossible: c_int,
        possible: *mut c_ulong,
    }

    /// XRROutputInfo
    #[repr(C)]
    struct OutputInfo {
        timestamp: c_ulong,
        crtc: c_ulong,
        name: *mut c_char,
        name_len: c_int,
        mm_width: c_ulong,
        mm_height: c_ulong,
        connection: u16,
        subpixel_order: u16,
        ncrtc: c_int,
        crtcs: *mut c_ulong,
        nclone: c_int,
        clones: *mut c_ulong,
        nmode: c_int,
        npreferred: c_int,
        modes: *mut c_ulong,
    }
    type XRRGetOutputInfo = unsafe extern "C" fn(*mut c_void, *mut ScreenResources, c_ulong) -> *mut OutputInfo;
    type XRRFreeOutputInfo = unsafe extern "C" fn(*mut OutputInfo);

    fn mode_hz(md: &ModeInfo) -> f32 {
        let t = md.h_total as f64 * md.v_total as f64;
        if t > 0.0 {
            (md.dot_clock as f64 / t) as f32
        } else {
            0.0
        }
    }

    /// The modes of the output that shows the primary monitor (all modes if that fails).
    pub fn modes() -> Option<Vec<super::Mode>> {
        let m = primary()?;
        let x11 = Lib::open(&[c"libX11.so.6", c"libX11.so"])?;
        let xr = Lib::open(&[c"libXrandr.so.2", c"libXrandr.so"])?;
        unsafe {
            let open: XOpenDisplay = x11.get(c"XOpenDisplay")?;
            let close: XCloseDisplay = x11.get(c"XCloseDisplay")?;
            let root_of: XDefaultRootWindow = x11.get(c"XDefaultRootWindow")?;
            let get_res: XRRGetScreenResourcesCurrent = xr.get(c"XRRGetScreenResourcesCurrent")?;
            let free_res: XRRFreeScreenResources = xr.get(c"XRRFreeScreenResources")?;
            let get_crtc: XRRGetCrtcInfo = xr.get(c"XRRGetCrtcInfo")?;
            let free_crtc: XRRFreeCrtcInfo = xr.get(c"XRRFreeCrtcInfo")?;
            let get_out: XRRGetOutputInfo = xr.get(c"XRRGetOutputInfo")?;
            let free_out: XRRFreeOutputInfo = xr.get(c"XRRFreeOutputInfo")?;
            let dpy = open(std::ptr::null());
            if dpy.is_null() {
                return None;
            }
            let res = get_res(dpy, root_of(dpy));
            if res.is_null() {
                close(dpy);
                return None;
            }
            let r = &*res;
            let all = std::slice::from_raw_parts(r.modes, r.nmode.max(0) as usize);
            let crtcs = std::slice::from_raw_parts(r.crtcs, r.ncrtc.max(0) as usize);
            let mut ids: Vec<c_ulong> = Vec::new();
            for &c in crtcs {
                let ci = get_crtc(dpy, res, c);
                if ci.is_null() {
                    continue;
                }
                let info = &*ci;
                if info.mode != 0 && info.x == m.x && info.y == m.y && info.noutput > 0 {
                    let out = *info.outputs;
                    let oi = get_out(dpy, res, out);
                    if !oi.is_null() {
                        ids.extend_from_slice(std::slice::from_raw_parts((*oi).modes, (*oi).nmode.max(0) as usize));
                        free_out(oi);
                    }
                }
                free_crtc(ci);
                if !ids.is_empty() {
                    break;
                }
            }
            let v: Vec<super::Mode> = all
                .iter()
                .filter(|md| ids.is_empty() || ids.contains(&md.id))
                .map(|md| super::Mode { w: md.width as i32, h: md.height as i32, hz: mode_hz(md) })
                .collect();
            free_res(res);
            close(dpy);
            Some(v)
        }
    }

    struct Lib(*mut c_void);
    impl Lib {
        fn open(names: &[&CStr]) -> Option<Lib> {
            for n in names {
                let h = unsafe { libc::dlopen(n.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
                if !h.is_null() {
                    return Some(Lib(h));
                }
            }
            None
        }
        unsafe fn get<T: Copy>(&self, name: &CStr) -> Option<T> {
            let p = libc::dlsym(self.0, name.as_ptr());
            if p.is_null() {
                None
            } else {
                Some(std::mem::transmute_copy::<*mut c_void, T>(&p))
            }
        }
    }

    pub fn primary() -> Option<Monitor> {
        let x11 = Lib::open(&[c"libX11.so.6", c"libX11.so"])?;
        unsafe {
            let open: XOpenDisplay = x11.get(c"XOpenDisplay")?;
            let close: XCloseDisplay = x11.get(c"XCloseDisplay")?;
            let root_of: XDefaultRootWindow = x11.get(c"XDefaultRootWindow")?;
            let screen_of: XDefaultScreen = x11.get(c"XDefaultScreen")?;
            let dw: XDisplayDim = x11.get(c"XDisplayWidth")?;
            let dh: XDisplayDim = x11.get(c"XDisplayHeight")?;
            let dpy = open(std::ptr::null());
            if dpy.is_null() {
                return None;
            }
            let root = root_of(dpy);
            let scr = screen_of(dpy);
            let mut m = Monitor { x: 0, y: 0, w: dw(dpy, scr), h: dh(dpy, scr), hz: 0.0 };
            if let Some(xr) = Lib::open(&[c"libXrandr.so.2", c"libXrandr.so"]) {
                if let (Some(get_mon), Some(free_mon)) = (xr.get::<XRRGetMonitors>(c"XRRGetMonitors"), xr.get::<XRRFreeMonitors>(c"XRRFreeMonitors")) {
                    let mut n = 0;
                    let mons = get_mon(dpy, root, 1, &mut n);
                    if !mons.is_null() && n > 0 {
                        let list = std::slice::from_raw_parts(mons, n as usize);
                        let pick = list.iter().find(|mi| mi.primary != 0).unwrap_or(&list[0]);
                        m.x = pick.x;
                        m.y = pick.y;
                        m.w = pick.width;
                        m.h = pick.height;
                    }
                    if !mons.is_null() {
                        free_mon(mons);
                    }
                }
                m.hz = refresh(&xr, dpy, root, m).unwrap_or(0.0);
            }
            close(dpy);
            Some(m)
        }
    }

    /// Refresh rate of the CRTC that shows the primary monitor's top-left corner.
    unsafe fn refresh(xr: &Lib, dpy: *mut c_void, root: c_ulong, m: Monitor) -> Option<f32> {
        let get_res: XRRGetScreenResourcesCurrent = xr.get(c"XRRGetScreenResourcesCurrent")?;
        let free_res: XRRFreeScreenResources = xr.get(c"XRRFreeScreenResources")?;
        let get_crtc: XRRGetCrtcInfo = xr.get(c"XRRGetCrtcInfo")?;
        let free_crtc: XRRFreeCrtcInfo = xr.get(c"XRRFreeCrtcInfo")?;
        let res = get_res(dpy, root);
        if res.is_null() {
            return None;
        }
        let r = &*res;
        let crtcs = std::slice::from_raw_parts(r.crtcs, r.ncrtc.max(0) as usize);
        let modes = std::slice::from_raw_parts(r.modes, r.nmode.max(0) as usize);
        let mut hz = None;
        for &c in crtcs {
            let ci = get_crtc(dpy, res, c);
            if ci.is_null() {
                continue;
            }
            let info = &*ci;
            if info.mode != 0 && info.x == m.x && info.y == m.y {
                if let Some(md) = modes.iter().find(|md| md.id == info.mode) {
                    let f = mode_hz(md);
                    if f > 0.0 {
                        hz = Some(f);
                    }
                }
            }
            free_crtc(ci);
            if hz.is_some() {
                break;
            }
        }
        free_res(res);
        hz
    }

    pub fn fullscreen(win: &minifb::Window) {
        use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
        let (Ok(wh), Ok(dh)) = (win.window_handle(), win.display_handle()) else { return };
        let (RawWindowHandle::Xlib(w), RawDisplayHandle::Xlib(d)) = (wh.as_raw(), dh.as_raw()) else { return };
        let Some(dpy) = d.display else { return };
        let dpy = dpy.as_ptr();
        let Some(x11) = Lib::open(&[c"libX11.so.6", c"libX11.so"]) else { return };
        unsafe {
            let (Some(atom), Some(send), Some(flush), Some(root_of)) = (
                x11.get::<XInternAtom>(c"XInternAtom"),
                x11.get::<XSendEvent>(c"XSendEvent"),
                x11.get::<XFlush>(c"XFlush"),
                x11.get::<XDefaultRootWindow>(c"XDefaultRootWindow"),
            ) else {
                return;
            };
            let state = atom(dpy, c"_NET_WM_STATE".as_ptr(), 0);
            let fs = atom(dpy, c"_NET_WM_STATE_FULLSCREEN".as_ptr(), 0);
            // XClientMessageEvent inside a zeroed XEvent (24 longs)
            let mut ev = [0 as c_long; 24];
            let base = ev.as_mut_ptr() as *mut u8;
            *(base as *mut c_int) = 33; // ClientMessage
            *(base.add(16) as *mut c_int) = 1; // send_event
            *(base.add(24) as *mut *mut c_void) = dpy;
            *(base.add(32) as *mut c_ulong) = w.window;
            *(base.add(40) as *mut c_ulong) = state;
            *(base.add(48) as *mut c_int) = 32; // format
            let data = base.add(56) as *mut c_long;
            *data = 1; // _NET_WM_STATE_ADD
            *data.add(1) = fs as c_long;
            *data.add(3) = 1; // normal application
            let mask: c_long = (1 << 20) | (1 << 19); // SubstructureRedirect | SubstructureNotify
            send(dpy, root_of(dpy), 0, mask, ev.as_mut_ptr());
            flush(dpy);
        }
    }
}

#[cfg(windows)]
mod win {
    use super::Monitor;
    use std::ffi::c_void;
    #[link(name = "user32")]
    extern "system" {
        fn SetProcessDPIAware() -> i32;
        fn GetDC(hwnd: *mut c_void) -> *mut c_void;
        fn ReleaseDC(hwnd: *mut c_void, hdc: *mut c_void) -> i32;
        fn GetSystemMetrics(i: i32) -> i32;
    }
    #[link(name = "gdi32")]
    extern "system" {
        fn GetDeviceCaps(hdc: *mut c_void, index: i32) -> i32;
    }
    /// DEVMODEW (display variant), 220 bytes
    #[repr(C)]
    struct DevMode {
        device_name: [u16; 32],
        spec_version: u16,
        driver_version: u16,
        size: u16,
        driver_extra: u16,
        fields: u32,
        position: [i32; 2],
        orientation: u32,
        fixed_output: u32,
        color: i16,
        duplex: i16,
        y_resolution: i16,
        tt_option: i16,
        collate: i16,
        form_name: [u16; 32],
        log_pixels: u16,
        bits_per_pel: u32,
        pels_width: u32,
        pels_height: u32,
        display_flags: u32,
        display_frequency: u32,
        icm: [u32; 8],
    }
    #[link(name = "user32")]
    extern "system" {
        fn EnumDisplaySettingsW(device: *const u16, mode: u32, dm: *mut DevMode) -> i32;
    }
    pub fn modes() -> Vec<super::Mode> {
        let mut v = Vec::new();
        unsafe {
            let mut dm: DevMode = std::mem::zeroed();
            dm.size = std::mem::size_of::<DevMode>() as u16;
            let mut i = 0;
            while EnumDisplaySettingsW(std::ptr::null(), i, &mut dm) != 0 {
                if dm.bits_per_pel >= 24 {
                    v.push(super::Mode { w: dm.pels_width as i32, h: dm.pels_height as i32, hz: dm.display_frequency as f32 });
                }
                i += 1;
                if i > 4000 {
                    break;
                }
            }
        }
        v
    }
    pub fn init() {
        unsafe {
            SetProcessDPIAware();
        }
    }
    pub fn primary() -> Monitor {
        unsafe {
            let mut m = Monitor { x: 0, y: 0, w: GetSystemMetrics(0), h: GetSystemMetrics(1), hz: 0.0 };
            let dc = GetDC(std::ptr::null_mut());
            if !dc.is_null() {
                let hz = GetDeviceCaps(dc, 116); // VREFRESH
                if hz > 1 {
                    m.hz = hz as f32;
                }
                let (dw, dh) = (GetDeviceCaps(dc, 118), GetDeviceCaps(dc, 117)); // DESKTOPHORZRES / VERTRES
                if dw > 0 && dh > 0 {
                    m.w = dw;
                    m.h = dh;
                }
                ReleaseDC(std::ptr::null_mut(), dc);
            }
            m
        }
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use super::Monitor;
    use std::ffi::c_void;
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGMainDisplayID() -> u32;
        fn CGDisplayPixelsWide(d: u32) -> usize;
        fn CGDisplayPixelsHigh(d: u32) -> usize;
        fn CGDisplayCopyDisplayMode(d: u32) -> *mut c_void;
        fn CGDisplayModeGetRefreshRate(m: *mut c_void) -> f64;
        fn CGDisplayModeRelease(m: *mut c_void);
        fn CGDisplayCopyAllDisplayModes(d: u32, options: *const c_void) -> *const c_void;
        fn CGDisplayModeGetWidth(m: *const c_void) -> usize;
        fn CGDisplayModeGetHeight(m: *const c_void) -> usize;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFArrayGetCount(a: *const c_void) -> isize;
        fn CFArrayGetValueAtIndex(a: *const c_void, i: isize) -> *const c_void;
        fn CFRelease(a: *const c_void);
    }
    pub fn modes() -> Vec<super::Mode> {
        let mut v = Vec::new();
        unsafe {
            let arr = CGDisplayCopyAllDisplayModes(CGMainDisplayID(), std::ptr::null());
            if arr.is_null() {
                return v;
            }
            for i in 0..CFArrayGetCount(arr) {
                let m = CFArrayGetValueAtIndex(arr, i);
                let hz = CGDisplayModeGetRefreshRate(m as *mut c_void) as f32;
                v.push(super::Mode { w: CGDisplayModeGetWidth(m) as i32, h: CGDisplayModeGetHeight(m) as i32, hz: if hz > 0.0 { hz } else { 60.0 } });
            }
            CFRelease(arr);
        }
        v
    }
    pub fn primary() -> Monitor {
        unsafe {
            let d = CGMainDisplayID();
            let mut m = Monitor { x: 0, y: 0, w: CGDisplayPixelsWide(d) as i32, h: CGDisplayPixelsHigh(d) as i32, hz: 0.0 };
            let mode = CGDisplayCopyDisplayMode(d);
            if !mode.is_null() {
                m.hz = CGDisplayModeGetRefreshRate(mode) as f32;
                CGDisplayModeRelease(mode);
            }
            m
        }
    }
}
