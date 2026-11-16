//! A small persistent worker pool for the rasterizer. A frame issues dozens of parallel row
//! jobs back to back (glass, fills, shadows, blits), so workers spin briefly between jobs
//! instead of sleeping, and the calling thread drains the queue too rather than waiting idle.
//! `run` returns only when every job of the batch has finished, which is what makes lending
//! borrowed data to the workers sound.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};

type Job = Box<dyn FnOnce() + Send + 'static>;

struct Shared {
    queue: Mutex<VecDeque<Job>>,
    ready: Condvar,
    queued: AtomicUsize,
}

struct Pool {
    shared: Arc<Shared>,
    size: usize,
}

static POOL: OnceLock<Pool> = OnceLock::new();

/// How long an idle worker keeps looking for work before it sleeps (spin iterations).
const SPIN: u32 = 20_000;

fn take(sh: &Shared) -> Option<Job> {
    if sh.queued.load(Ordering::Acquire) == 0 {
        return None;
    }
    let mut q = sh.queue.lock().ok()?;
    let j = q.pop_front();
    if j.is_some() {
        sh.queued.fetch_sub(1, Ordering::AcqRel);
    }
    j
}

fn pool() -> &'static Pool {
    POOL.get_or_init(|| {
        // the calling thread works too, so one fewer worker than cores
        let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).clamp(1, 16);
        let shared = Arc::new(Shared { queue: Mutex::new(VecDeque::new()), ready: Condvar::new(), queued: AtomicUsize::new(0) });
        for i in 0..cores.saturating_sub(1) {
            let sh = shared.clone();
            let _ = std::thread::Builder::new().name(format!("raster-{}", i)).spawn(move || loop {
                let mut job = None;
                for _ in 0..SPIN {
                    job = take(&sh);
                    if job.is_some() {
                        break;
                    }
                    std::hint::spin_loop();
                }
                let job = match job {
                    Some(j) => j,
                    None => {
                        let Ok(mut q) = sh.queue.lock() else { return };
                        loop {
                            if let Some(j) = q.pop_front() {
                                sh.queued.fetch_sub(1, Ordering::AcqRel);
                                break j;
                            }
                            q = match sh.ready.wait(q) {
                                Ok(q) => q,
                                Err(_) => return,
                            };
                        }
                    }
                };
                job();
            });
        }
        Pool { shared, size: cores }
    })
}

/// Number of threads that share a batch (workers plus the caller).
pub fn size() -> usize {
    pool().size
}

/// Run every job and wait for all of them. Only one thread (the renderer) submits batches.
pub fn run<'a>(jobs: Vec<Box<dyn FnOnce() + Send + 'a>>) {
    let n = jobs.len();
    if n == 0 {
        return;
    }
    let p = pool();
    let left = Arc::new(AtomicUsize::new(n));
    {
        let mut q = p.shared.queue.lock().unwrap();
        for job in jobs {
            let left = left.clone();
            let wrapped: Box<dyn FnOnce() + Send + 'a> = Box::new(move || {
                // a panicking job must still count as done, or `run` would wait forever
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(job));
                left.fetch_sub(1, Ordering::AcqRel);
            });
            // SAFETY: `run` does not return until `left` reaches zero, i.e. until every job
            // (which may borrow data living in the caller's frame) has finished running.
            let job: Job = unsafe { std::mem::transmute::<Box<dyn FnOnce() + Send + 'a>, Job>(wrapped) };
            q.push_back(job);
        }
        p.shared.queued.fetch_add(n, Ordering::AcqRel);
    }
    p.shared.ready.notify_all();
    // help out, then wait for stragglers
    while left.load(Ordering::Acquire) > 0 {
        match take(&p.shared) {
            Some(j) => j(),
            None => std::hint::spin_loop(),
        }
    }
}
