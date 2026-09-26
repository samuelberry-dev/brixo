//! Slowing down abuse: how often one address (or account) may try
//! something within a stretch of time. Kept in memory: a restart forgets,
//! which is fine for this.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// The limits the website uses: (most tries, within this long).
pub const LOGIN_FAILS: (usize, Duration) = (10, Duration::from_secs(15 * 60));
pub const SIGNUPS: (usize, Duration) = (5, Duration::from_secs(60 * 60));
pub const BAD_INVITES: (usize, Duration) = (10, Duration::from_secs(15 * 60));
pub const PLAYS: (usize, Duration) = (30, Duration::from_secs(60));
pub const PUBLISHES: (usize, Duration) = (30, Duration::from_secs(60 * 60));

#[derive(Default)]
pub struct Limiter(Mutex<HashMap<String, VecDeque<Instant>>>);

impl Limiter {
    /// True if `key` has had fewer than `max` hits within `window`.
    pub fn ok(&self, key: &str, (max, window): (usize, Duration)) -> bool {
        let mut map = self.0.lock().unwrap();
        match map.get_mut(key) {
            Some(hits) => {
                forget_old(hits, window);
                hits.len() < max
            }
            None => true,
        }
    }

    /// Counts one hit for `key`.
    pub fn hit(&self, key: &str) {
        let mut map = self.0.lock().unwrap();
        if map.len() > 50_000 {
            // Tidy up now and then so the map can't grow forever.
            map.retain(|_, hits| hits.back().is_some_and(|t| t.elapsed() < Duration::from_secs(2 * 60 * 60)));
        }
        map.entry(key.to_string()).or_default().push_back(Instant::now());
    }

    /// `ok`, then counts the hit if it was allowed.
    pub fn take(&self, key: &str, limit: (usize, Duration)) -> bool {
        let allowed = self.ok(key, limit);
        if allowed {
            self.hit(key);
        }
        allowed
    }
}

fn forget_old(hits: &mut VecDeque<Instant>, window: Duration) {
    while hits.front().is_some_and(|t| t.elapsed() >= window) {
        hits.pop_front();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_within_the_window_only() {
        let l = Limiter::default();
        let three = (3, Duration::from_millis(200));
        assert!(l.take("a", three) && l.take("a", three) && l.take("a", three));
        assert!(!l.take("a", three), "fourth is refused");
        assert!(l.take("b", three), "other keys are separate");
        std::thread::sleep(Duration::from_millis(250));
        assert!(l.take("a", three), "old hits are forgotten");
    }
}
