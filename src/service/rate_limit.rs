//! Simple in-process login rate limiting by account and client IP.
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use lazy_static::lazy_static;

const WINDOW: Duration = Duration::from_secs(15 * 60);
const MAX_FAILURES: usize = 5;

#[derive(Default)]
struct Bucket {
    failures: Vec<Instant>,
}

impl Bucket {
    fn prune(&mut self, now: Instant) {
        self.failures.retain(|at| now.duration_since(*at) < WINDOW);
    }

    fn is_blocked(&mut self, now: Instant) -> bool {
        self.prune(now);
        self.failures.len() >= MAX_FAILURES
    }

    fn record_failure(&mut self, now: Instant) {
        self.prune(now);
        self.failures.push(now);
    }

    fn clear(&mut self) {
        self.failures.clear();
    }
}

#[derive(Default)]
pub struct LoginRateLimiter {
    by_account: HashMap<String, Bucket>,
    by_ip: HashMap<String, Bucket>,
}

impl LoginRateLimiter {
    pub fn check(&mut self, username: &str, ip: &str) -> Result<(), String> {
        let now = Instant::now();
        let account_blocked = self
            .by_account
            .entry(username.to_ascii_lowercase())
            .or_default()
            .is_blocked(now);
        let ip_blocked = self.by_ip.entry(ip.to_owned()).or_default().is_blocked(now);
        if account_blocked || ip_blocked {
            return Err("Too many login attempts; try again later".into());
        }
        Ok(())
    }

    pub fn record_failure(&mut self, username: &str, ip: &str) {
        let now = Instant::now();
        self.by_account
            .entry(username.to_ascii_lowercase())
            .or_default()
            .record_failure(now);
        self.by_ip.entry(ip.to_owned()).or_default().record_failure(now);
    }

    pub fn clear_success(&mut self, username: &str, ip: &str) {
        if let Some(bucket) = self.by_account.get_mut(&username.to_ascii_lowercase()) {
            bucket.clear();
        }
        if let Some(bucket) = self.by_ip.get_mut(ip) {
            bucket.clear();
        }
    }
}

lazy_static! {
    pub static ref LOGIN_RATE_LIMITER: Mutex<LoginRateLimiter> =
        Mutex::new(LoginRateLimiter::default());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_after_max_failures_per_account() {
        let mut limiter = LoginRateLimiter::default();
        for _ in 0..MAX_FAILURES {
            assert!(limiter.check("alice", "10.0.0.1").is_ok());
            limiter.record_failure("alice", "10.0.0.1");
        }
        assert!(limiter.check("alice", "10.0.0.1").is_err());
        // Independent account still allowed from same IP until IP bucket fills.
        assert!(limiter.check("bob", "10.0.0.2").is_ok());
        limiter.clear_success("alice", "10.0.0.1");
        assert!(limiter.check("alice", "10.0.0.1").is_ok());
    }
}
