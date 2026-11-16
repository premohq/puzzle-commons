//! Tiny frame profiler for `--bench`: named sections accumulate wall time on this thread.
//! Off unless enabled, so normal play pays one thread-local bool check per section.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Instant;

thread_local! {
    static ON: RefCell<bool> = const { RefCell::new(false) };
    static ACC: RefCell<HashMap<&'static str, (f64, u32)>> = RefCell::new(HashMap::new());
}

pub fn enable(on: bool) {
    ON.with(|o| *o.borrow_mut() = on);
    ACC.with(|a| a.borrow_mut().clear());
}

pub struct Guard(Option<(&'static str, Instant)>);
impl Drop for Guard {
    fn drop(&mut self) {
        if let Some((n, t)) = self.0.take() {
            let dt = t.elapsed().as_secs_f64();
            ACC.with(|a| {
                let mut a = a.borrow_mut();
                let e = a.entry(n).or_insert((0.0, 0));
                e.0 += dt;
                e.1 += 1;
            });
        }
    }
}

#[inline]
pub fn scope(name: &'static str) -> Guard {
    if ON.with(|o| *o.borrow()) {
        Guard(Some((name, Instant::now())))
    } else {
        Guard(None)
    }
}

/// Print and clear: per-frame milliseconds over `frames` frames, biggest first.
pub fn report(frames: u32) {
    ACC.with(|a| {
        let mut v: Vec<(&str, (f64, u32))> = a.borrow().iter().map(|(k, v)| (*k, *v)).collect();
        v.sort_by(|x, y| y.1 .0.partial_cmp(&x.1 .0).unwrap());
        for (n, (t, c)) in v.iter().take(14) {
            println!("      {:16} {:6.2} ms  ({} calls/frame)", n, t * 1000.0 / frames as f64, c / frames.max(1));
        }
        a.borrow_mut().clear();
    });
}
