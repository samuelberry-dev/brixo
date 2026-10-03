//! A pretend bad connection, for testing how the game feels far from the
//! server. Brixo Player turns it on with BRIXO_LAG:
//!
//!   BRIXO_LAG=150          150 ms ping (75 each way)
//!   BRIXO_LAG=150,30       plus up to 30 ms of jitter each way
//!   BRIXO_LAG=150,30,2     plus a 2% chance per message of a stall: what a
//!                          lost packet looks like over TCP (it's resent, and
//!                          everything behind it waits)
//!
//! Messages keep their order, like real TCP: a held one holds up the rest.

use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lag {
    /// Round trip, in milliseconds (half each way).
    pub ping_ms: f32,
    /// Up to this much extra each way, at random.
    pub jitter_ms: f32,
    /// Percent of messages that stall (a lost packet being resent).
    pub stall_percent: f32,
}

impl Lag {
    /// From BRIXO_LAG, if it's set (and makes sense).
    pub fn from_env() -> Option<Lag> {
        Lag::parse(&std::env::var("BRIXO_LAG").ok()?)
    }

    pub fn parse(text: &str) -> Option<Lag> {
        let nums: Vec<f32> = text.split(',').map(|n| n.trim().parse::<f32>()).collect::<Result<_, _>>().ok()?;
        let lag = Lag {
            ping_ms: *nums.first()?,
            jitter_ms: nums.get(1).copied().unwrap_or(0.0),
            stall_percent: nums.get(2).copied().unwrap_or(0.0),
        };
        (lag.ping_ms >= 0.0 && lag.jitter_ms >= 0.0 && (0.0..=100.0).contains(&lag.stall_percent)).then_some(lag)
    }
}

/// Decides when each message in one direction gets through.
pub struct Delayer {
    lag: Lag,
    /// The last message's arrival: nothing overtakes it.
    last: Option<Instant>,
    seed: u64,
}

impl Delayer {
    pub fn new(lag: Lag, seed: u64) -> Delayer {
        Delayer { lag, last: None, seed: seed | 1 }
    }

    /// 0 to 1, from a little xorshift (no need for anything better).
    fn random(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        (self.seed >> 40) as f32 / (1u64 << 24) as f32
    }

    /// When a message sent at `now` arrives.
    pub fn arrival(&mut self, now: Instant) -> Instant {
        let one_way = self.lag.ping_ms / 2.0;
        let mut ms = one_way + self.random() * self.lag.jitter_ms;
        if self.random() * 100.0 < self.lag.stall_percent {
            // A resend: at least a couple of round trips, 200 ms minimum
            // (TCP's shortest retry).
            ms += (self.lag.ping_ms * 2.0).max(200.0);
        }
        let at = now + Duration::from_secs_f32(ms / 1000.0);
        let at = match self.last {
            Some(last) if last > at => last,
            _ => at,
        };
        self.last = Some(at);
        at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_setting() {
        assert_eq!(Lag::parse("150"), Some(Lag { ping_ms: 150.0, jitter_ms: 0.0, stall_percent: 0.0 }));
        assert_eq!(Lag::parse("150, 30, 2"), Some(Lag { ping_ms: 150.0, jitter_ms: 30.0, stall_percent: 2.0 }));
        assert_eq!(Lag::parse("lots"), None);
        assert_eq!(Lag::parse("-5"), None);
    }

    #[test]
    fn delays_keep_their_order() {
        let mut d = Delayer::new(Lag { ping_ms: 100.0, jitter_ms: 80.0, stall_percent: 10.0 }, 42);
        let start = Instant::now();
        let mut last = start;
        let mut stalls = 0;
        for i in 0..1000 {
            let now = start + Duration::from_millis(i * 16);
            let at = d.arrival(now);
            assert!(at >= last, "nothing overtakes");
            assert!(at >= now + Duration::from_millis(50), "at least half the ping");
            if at > now + Duration::from_millis(50 + 80 + 150) {
                stalls += 1;
            }
            last = at;
        }
        assert!(stalls > 20, "some stalls: {stalls}");
    }
}
