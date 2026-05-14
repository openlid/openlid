//! 15-second idle timer. The helper calls `arm()` on every client
//! disconnect; `disarm()` on every client connect. If `arm()` is followed
//! by 15 s of no `disarm()`, the timer fires and we exit.

use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const IDLE_EXIT_SECS: u64 = 15;

#[derive(Clone)]
pub struct IdleExit {
    inner: Arc<Mutex<State>>,
}

struct State {
    generation: u64,
    armed: bool,
}

impl IdleExit {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(State {
                generation: 0,
                armed: false,
            })),
        }
    }

    pub fn arm<F: FnOnce() + Send + 'static>(&self, on_fire: F) {
        let mut state = self.inner.lock().unwrap();
        state.generation = state.generation.wrapping_add(1);
        state.armed = true;
        let my_gen = state.generation;
        drop(state);

        let inner = Arc::clone(&self.inner);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(IDLE_EXIT_SECS));
            let s = inner.lock().unwrap();
            if s.armed && s.generation == my_gen {
                drop(s);
                on_fire();
            }
        });
    }

    pub fn disarm(&self) {
        self.inner.lock().unwrap().armed = false;
    }

    #[cfg(test)]
    pub fn is_armed(&self) -> bool {
        self.inner.lock().unwrap().armed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Instant;

    impl IdleExit {
        fn arm_for_test<F: FnOnce() + Send + 'static>(&self, dur: Duration, on_fire: F) {
            let mut state = self.inner.lock().unwrap();
            state.generation = state.generation.wrapping_add(1);
            state.armed = true;
            let my_gen = state.generation;
            drop(state);
            let inner = Arc::clone(&self.inner);
            std::thread::spawn(move || {
                std::thread::sleep(dur);
                let s = inner.lock().unwrap();
                if s.armed && s.generation == my_gen {
                    drop(s);
                    on_fire();
                }
            });
        }
    }

    /// Poll `predicate` until it returns true or the deadline is hit. Returns
    /// whether the predicate became true. Used in place of `sleep(longer than
    /// expected) + assert`, which flakes on CI when the scheduler delays the
    /// timer thread by more than the test's headroom window.
    fn wait_for<F: Fn() -> bool>(predicate: F, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if predicate() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        predicate()
    }

    #[test]
    fn fires_after_duration_if_not_disarmed() {
        let fired = Arc::new(AtomicBool::new(false));
        let f2 = Arc::clone(&fired);
        let t = IdleExit::new();
        t.arm_for_test(Duration::from_millis(50), move || {
            f2.store(true, Ordering::SeqCst);
        });
        assert!(
            wait_for(|| fired.load(Ordering::SeqCst), Duration::from_secs(2)),
            "timer did not fire within 2s",
        );
    }

    #[test]
    fn disarm_before_fire_prevents_firing() {
        let fired = Arc::new(AtomicBool::new(false));
        let f2 = Arc::clone(&fired);
        let t = IdleExit::new();
        t.arm_for_test(Duration::from_millis(100), move || {
            f2.store(true, Ordering::SeqCst);
        });
        // Disarm before sleeping so CI scheduler pauses can't reorder
        // arm/disarm around the timer thread's wakeup.
        t.disarm();
        // Wait well beyond the timer duration; the closure must NOT fire.
        std::thread::sleep(Duration::from_millis(300));
        assert!(!fired.load(Ordering::SeqCst));
    }

    #[test]
    fn rearm_supersedes_previous() {
        let fired_first = Arc::new(AtomicBool::new(false));
        let f1 = Arc::clone(&fired_first);
        let fired_second = Arc::new(AtomicBool::new(false));
        let f2 = Arc::clone(&fired_second);

        let t = IdleExit::new();
        t.arm_for_test(Duration::from_millis(50), move || {
            f1.store(true, Ordering::SeqCst);
        });
        let start = Instant::now();
        // Re-arm immediately. The first timer's generation is invalidated,
        // so even if its sleep elapses on a delayed scheduler, the closure
        // observes the bumped generation and exits without firing.
        t.arm_for_test(Duration::from_millis(150), move || {
            f2.store(true, Ordering::SeqCst);
        });
        assert!(
            wait_for(
                || fired_second.load(Ordering::SeqCst),
                Duration::from_secs(2)
            ),
            "second timer did not fire within 2s; elapsed: {:?}",
            start.elapsed(),
        );
        // The first timer's closure should NEVER have fired.
        assert!(
            !fired_first.load(Ordering::SeqCst),
            "first (superseded) timer fired anyway",
        );
    }
}
