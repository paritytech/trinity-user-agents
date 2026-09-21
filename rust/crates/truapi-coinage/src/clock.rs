// SPDX-License-Identifier: AGPL-3.0-only
// Derived from paritytech/brevity-dozer, core/crates/brevity-coinage.
// Copyright the Brevity contributors. See NOTICE and LICENSE in this crate.

//! The injected wall clock (unix milliseconds, the crate-wide timestamp
//! convention) — services never read system time directly, so tests pin
//! `now` deterministically.

/// Milliseconds since the unix epoch.
pub trait Clock: Send + Sync {
    fn now_ms(&self) -> i64;
}

/// Production clock.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }
}

/// Fixed clock for tests.
pub struct FixedClock(pub i64);

impl Clock for FixedClock {
    fn now_ms(&self) -> i64 {
        self.0
    }
}
