//! Simple in-process login rate limiting by account and client IP.
//!
//! The per-account bucket is the hard anti-brute-force control: it cannot be
//! spoofed by headers. The per-IP bucket is best-effort slowdown — `client_ip`
//! may honor forwarded headers behind a reverse proxy, so treat it as
//! throttling, not a security boundary. Both maps are size-capped so
//! unauthenticated traffic cannot grow memory without bound.
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use lazy_static::lazy_static;

const WINDOW: Duration = Duration::from_secs(15 * 60);
const MAX_FAILURES: usize = 5;
const MAX_BUCKETS: usize = 10_000;

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

/// Per-IP failures remember which account caused them, so one successful
/// login only clears its own entries instead of the whole shared bucket.
#[derive(Default)]
struct IpBucket {
    failures: Vec<(String, Instant)>,
}

impl IpBucket {
    fn prune(&mut self, now: Instant) {
        self.failures.retain(|(_, at)| now.duration_since(*at) < WINDOW);
    }

    fn is_blocked(&mut self, now: Instant) -> bool {
        self.prune(now);
        self.failures.len() >= MAX_FAILURES
    }

    fn record_failure(&mut self, account: &str, now: Instant) {
        self.prune(now);
        self.failures.push((account.to_owned(), now));
    }

    fn clear_account(&mut self, account: &str) {
        self.failures.retain(|(owner, _)| owner != account);
    }
}

trait Evictable {
    fn prune_expired(&mut self, now: Instant);
    fn is_empty(&self) -> bool;
    fn last_failure(&self) -> Option<Instant>;
}

impl Evictable for Bucket {
    fn prune_expired(&mut self, now: Instant) {
        self.prune(now);
    }
    fn is_empty(&self) -> bool {
        self.failures.is_empty()
    }
    fn last_failure(&self) -> Option<Instant> {
        self.failures.last().copied()
    }
}

impl Evictable for IpBucket {
    fn prune_expired(&mut self, now: Instant) {
        self.prune(now);
    }
    fn is_empty(&self) -> bool {
        self.failures.is_empty()
    }
    fn last_failure(&self) -> Option<Instant> {
        self.failures.last().map(|(_, at)| *at)
    }
}

/// Keep the map bounded: drop expired entries first, then evict the
/// longest-idle buckets. Only runs once the cap is exceeded.
fn evict_if_needed<V: Evictable>(map: &mut HashMap<String, V>, now: Instant) {
    if map.len() <= MAX_BUCKETS {
        return;
    }
    for bucket in map.values_mut() {
        bucket.prune_expired(now);
    }
    map.retain(|_, bucket| !bucket.is_empty());
    if map.len() <= MAX_BUCKETS {
        return;
    }
    let mut idle_order: Vec<(Instant, String)> = map
        .iter()
        .filter_map(|(key, bucket)| bucket.last_failure().map(|at| (at, key.clone())))
        .collect();
    idle_order.sort_by_key(|(at, _)| *at);
    let excess = map.len().saturating_sub(MAX_BUCKETS * 3 / 4);
    for (_, key) in idle_order.into_iter().take(excess) {
        map.remove(&key);
    }
}

#[derive(Default)]
pub struct LoginRateLimiter {
    by_account: HashMap<String, Bucket>,
    by_ip: HashMap<String, IpBucket>,
}

impl LoginRateLimiter {
    /// Note: `check` never allocates, so probing it with random usernames or
    /// spoofed IPs cannot grow the maps.
    pub fn check(&mut self, username: &str, ip: &str) -> Result<(), String> {
        let now = Instant::now();
        let account_key = username.to_ascii_lowercase();
        let account_blocked = self
            .by_account
            .get_mut(&account_key)
            .map(|bucket| bucket.is_blocked(now))
            .unwrap_or(false);
        let ip_blocked = self
            .by_ip
            .get_mut(ip)
            .map(|bucket| bucket.is_blocked(now))
            .unwrap_or(false);
        if account_blocked || ip_blocked {
            return Err("Too many login attempts; try again later".into());
        }
        Ok(())
    }

    pub fn record_failure(&mut self, username: &str, ip: &str) {
        let now = Instant::now();
        evict_if_needed(&mut self.by_account, now);
        evict_if_needed(&mut self.by_ip, now);
        let account_key = username.to_ascii_lowercase();
        self.by_account
            .entry(account_key.clone())
            .or_default()
            .record_failure(now);
        self.by_ip
            .entry(ip.to_owned())
            .or_default()
            .record_failure(&account_key, now);
    }

    pub fn clear_success(&mut self, username: &str, ip: &str) {
        let account_key = username.to_ascii_lowercase();
        if let Some(bucket) = self.by_account.get_mut(&account_key) {
            bucket.clear();
        }
        if let Some(bucket) = self.by_ip.get_mut(ip) {
            bucket.clear_account(&account_key);
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

    #[test]
    fn check_does_not_allocate_buckets() {
        let mut limiter = LoginRateLimiter::default();
        for i in 0..50_000 {
            let username = format!("ghost-{i}");
            assert!(limiter.check(&username, &format!("198.51.100.{}", i % 254)).is_ok());
        }
        assert!(
            limiter.by_account.is_empty(),
            "check() must not create account buckets"
        );
        assert!(
            limiter.by_ip.is_empty(),
            "check() must not create IP buckets"
        );
    }

    #[test]
    fn bucket_maps_are_capped() {
        let mut limiter = LoginRateLimiter::default();
        for i in 0..(MAX_BUCKETS + 500) {
            limiter.record_failure(&format!("account-{i}"), &format!("203.0.113.{}", i % 254));
        }
        assert!(limiter.by_account.len() <= MAX_BUCKETS);
        assert!(limiter.by_ip.len() <= MAX_BUCKETS);
        // Recently inserted buckets survive eviction.
        assert!(limiter
            .by_account
            .contains_key(&format!("account-{}", MAX_BUCKETS + 499)));
    }

    #[test]
    fn success_clears_only_that_accounts_ip_entries() {
        let mut limiter = LoginRateLimiter::default();
        // Four failed attempts for throwaway accounts share the IP bucket.
        for i in 0..4 {
            let name = format!("fake-{i}");
            assert!(limiter.check(&name, "192.0.2.10").is_ok());
            limiter.record_failure(&name, "192.0.2.10");
        }
        // An unrelated legitimate account signs in from that IP.
        limiter.clear_success("alice", "192.0.2.10");
        // The four failures from the fake accounts must still count: one more
        // failure from a new name trips the shared IP bucket.
        limiter.record_failure("fake-4", "192.0.2.10");
        assert!(
            limiter.check("carol", "192.0.2.10").is_err(),
            "successful login must not reset other accounts' IP failures"
        );
    }
}
