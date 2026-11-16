//! Local wall-clock time for the persistent UI clock (no crates: libc on Unix, Win32 on Windows).

/// (hour 0-23, minute)
pub fn local_hm() -> (u32, u32) {
    imp::local_hm()
}

pub fn text() -> String {
    let (h, m) = local_hm();
    format!("{:02}:{:02}", h, m)
}

#[cfg(unix)]
mod imp {
    pub fn local_hm() -> (u32, u32) {
        unsafe {
            let t = libc::time(std::ptr::null_mut());
            let mut tm: libc::tm = std::mem::zeroed();
            if libc::localtime_r(&t, &mut tm).is_null() {
                return (0, 0);
            }
            (tm.tm_hour as u32, tm.tm_min as u32)
        }
    }
}

#[cfg(windows)]
#[allow(non_snake_case)]
mod imp {
    #[repr(C)]
    struct SYSTEMTIME {
        wYear: u16,
        wMonth: u16,
        wDayOfWeek: u16,
        wDay: u16,
        wHour: u16,
        wMinute: u16,
        wSecond: u16,
        wMilliseconds: u16,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetLocalTime(st: *mut SYSTEMTIME);
    }
    pub fn local_hm() -> (u32, u32) {
        let mut st = SYSTEMTIME { wYear: 0, wMonth: 0, wDayOfWeek: 0, wDay: 0, wHour: 0, wMinute: 0, wSecond: 0, wMilliseconds: 0 };
        unsafe { GetLocalTime(&mut st) };
        (st.wHour as u32, st.wMinute as u32)
    }
}

#[cfg(not(any(unix, windows)))]
mod imp {
    pub fn local_hm() -> (u32, u32) {
        let s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        (((s / 3600) % 24) as u32, ((s / 60) % 60) as u32)
    }
}
