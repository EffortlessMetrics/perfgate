//! Fake clock for deterministic time-based testing.

use std::sync::{Arc, Mutex};
use std::time::Duration;

/// A fake clock that returns configurable timestamps.
///
/// This is useful for testing code that depends on timing without
/// introducing non-determinism from real system clocks.
///
/// # Thread Safety
///
/// All methods are `&self` (not `&mut self`), making it safe
/// to share a single instance across multiple threads in tests.
///
/// # Example
///
/// ```
/// use perfgate_fake::FakeClock;
/// use std::time::Duration;
///
/// let clock = FakeClock::new()
///     .with_millis(1000);
///
/// assert_eq!(clock.now_millis(), 1000);
///
/// clock.advance(Duration::from_millis(500));
/// assert_eq!(clock.now_millis(), 1500);
/// ```
#[derive(Debug, Clone)]
pub struct FakeClock {
    inner: Arc<Mutex<FakeClockInner>>,
}

#[derive(Debug, Clone)]
struct FakeClockInner {
    millis: u64,
}

impl Default for FakeClock {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeClock {
    /// Create a new `FakeClock` starting at time 0.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(FakeClockInner { millis: 0 })),
        }
    }

    /// Create a `FakeClock` starting at a specific time in milliseconds.
    pub fn at_millis(millis: u64) -> Self {
        Self {
            inner: Arc::new(Mutex::new(FakeClockInner { millis })),
        }
    }

    /// Create a `FakeClock` starting at a specific time.
    pub fn at(duration: Duration) -> Self {
        Self::at_millis(duration.as_millis() as u64)
    }

    /// Set the current time in milliseconds.
    pub fn with_millis(self, millis: u64) -> Self {
        self.inner.lock().expect("lock").millis = millis;
        self
    }

    /// Set the current time.
    pub fn with_duration(self, duration: Duration) -> Self {
        self.with_millis(duration.as_millis() as u64)
    }

    /// Get the current time in milliseconds.
    pub fn now_millis(&self) -> u64 {
        self.inner.lock().expect("lock").millis
    }

    /// Get the current time as a `Duration`.
    pub fn now(&self) -> Duration {
        Duration::from_millis(self.now_millis())
    }

    /// Advance the clock by a duration.
    pub fn advance(&self, duration: Duration) {
        self.advance_millis(duration.as_millis() as u64);
    }

    /// Advance the clock by a number of milliseconds.
    pub fn advance_millis(&self, millis: u64) {
        let mut inner = self.inner.lock().expect("lock");
        inner.millis = inner.millis.saturating_add(millis);
    }

    /// Reset the clock to time 0.
    pub fn reset(&self) {
        self.inner.lock().expect("lock").millis = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_clock_starts_at_zero() {
        let clock = FakeClock::new();
        assert_eq!(clock.now_millis(), 0);
    }

    #[test]
    fn at_millis_sets_initial_time() {
        let clock = FakeClock::at_millis(5000);
        assert_eq!(clock.now_millis(), 5000);
    }

    #[test]
    fn at_duration_sets_initial_time() {
        let clock = FakeClock::at(Duration::from_secs(10));
        assert_eq!(clock.now_millis(), 10000);
    }

    #[test]
    fn with_millis_configures_time() {
        let clock = FakeClock::new().with_millis(1234);
        assert_eq!(clock.now_millis(), 1234);
    }

    #[test]
    fn with_duration_configures_time() {
        let clock = FakeClock::new().with_duration(Duration::from_secs(30));
        assert_eq!(clock.now_millis(), 30000);
    }

    #[test]
    fn advance_increments_time() {
        let clock = FakeClock::new().with_millis(100);
        clock.advance(Duration::from_millis(50));

        assert_eq!(clock.now_millis(), 150);
    }

    #[test]
    fn advance_millis_increments_time() {
        let clock = FakeClock::new().with_millis(100);
        clock.advance_millis(200);

        assert_eq!(clock.now_millis(), 300);
    }

    #[test]
    fn reset_returns_to_zero() {
        let clock = FakeClock::new().with_millis(9999);
        clock.reset();

        assert_eq!(clock.now_millis(), 0);
    }

    #[test]
    fn saturating_addition_on_overflow() {
        let clock = FakeClock::new().with_millis(u64::MAX);
        clock.advance_millis(1);

        assert_eq!(clock.now_millis(), u64::MAX);
    }

    #[test]
    fn thread_safe_sharing() {
        use std::sync::Arc;
        use std::thread;

        let clock = Arc::new(FakeClock::new());

        let handles: Vec<_> = (0..4)
            .map(|_i| {
                let c = clock.clone();
                thread::spawn(move || {
                    c.advance_millis(100);
                    c.now_millis()
                })
            })
            .collect();

        let _results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

        assert_eq!(clock.now_millis(), 400);
    }
}
