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

    #[test]
    fn fires_after_duration_if_not_disarmed() {
        let fired = Arc::new(AtomicBool::new(false));
        let f2 = Arc::clone(&fired);
        let t = IdleExit::new();
        t.arm_for_test(Duration::from_millis(50), move || {
            f2.store(true, Ordering::SeqCst);
        });
        std::thread::sleep(Duration::from_millis(120));
        assert!(fired.load(Ordering::SeqCst));
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
        std::thread::sleep(Duration::from_millis(200));
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
        std::thread::sleep(Duration::from_millis(10));
        t.arm_for_test(Duration::from_millis(150), move || {
            f2.store(true, Ordering::SeqCst);
        });
        std::thread::sleep(Duration::from_millis(80));
        assert!(!fired_first.load(Ordering::SeqCst));
        std::thread::sleep(Duration::from_millis(120));
        assert!(
            fired_second.load(Ordering::SeqCst),
            "elapsed: {:?}",
            start.elapsed()
        );
    }
}
