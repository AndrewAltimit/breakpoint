use std::collections::HashMap;
use std::net::IpAddr;
use std::time::Instant;

use tokio::sync::Mutex;

/// Per-IP token bucket for rate limiting.
struct TokenBucket {
    tokens: f64,
    last_refill: Instant,
}

/// IP-based rate limiter using token bucket algorithm.
pub struct IpRateLimiter {
    buckets: Mutex<HashMap<IpAddr, TokenBucket>>,
    max_tokens: f64,
    refill_rate: f64, // tokens per second
}

impl IpRateLimiter {
    pub fn new(max_tokens: f64, refill_rate: f64) -> Self {
        Self {
            buckets: Mutex::new(HashMap::new()),
            max_tokens,
            refill_rate,
        }
    }

    /// Returns `true` if the request is allowed, `false` if rate-limited.
    pub async fn check_rate_limit(&self, ip: IpAddr) -> bool {
        let mut buckets = self.buckets.lock().await;
        let now = Instant::now();
        let bucket = buckets.entry(ip).or_insert_with(|| TokenBucket {
            tokens: self.max_tokens,
            last_refill: now,
        });

        // Refill
        let elapsed = now.duration_since(bucket.last_refill).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * self.refill_rate).min(self.max_tokens);
        bucket.last_refill = now;

        // Consume
        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// Remove stale entries that haven't been accessed in the given duration.
    pub async fn cleanup(&self, max_age: std::time::Duration) {
        let mut buckets = self.buckets.lock().await;
        let now = Instant::now();
        buckets.retain(|_, bucket| now.duration_since(bucket.last_refill) < max_age);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn allows_requests_within_limit() {
        let limiter = IpRateLimiter::new(5.0, 5.0);
        let ip: IpAddr = "127.0.0.1".parse().unwrap();
        for _ in 0..5 {
            assert!(limiter.check_rate_limit(ip).await);
        }
    }

    #[tokio::test]
    async fn rejects_requests_over_limit() {
        let limiter = IpRateLimiter::new(3.0, 0.0); // no refill
        let ip: IpAddr = "127.0.0.1".parse().unwrap();
        assert!(limiter.check_rate_limit(ip).await);
        assert!(limiter.check_rate_limit(ip).await);
        assert!(limiter.check_rate_limit(ip).await);
        assert!(!limiter.check_rate_limit(ip).await);
    }

    #[tokio::test]
    async fn separate_buckets_per_ip() {
        let limiter = IpRateLimiter::new(1.0, 0.0); // 1 token, no refill
        let ip1: IpAddr = "10.0.0.1".parse().unwrap();
        let ip2: IpAddr = "10.0.0.2".parse().unwrap();
        assert!(limiter.check_rate_limit(ip1).await);
        assert!(!limiter.check_rate_limit(ip1).await);
        // ip2 has its own bucket
        assert!(limiter.check_rate_limit(ip2).await);
        assert!(!limiter.check_rate_limit(ip2).await);
    }

    #[tokio::test]
    async fn refills_over_time() {
        let limiter = IpRateLimiter::new(2.0, 100.0); // 100 tokens/sec refill
        let ip: IpAddr = "127.0.0.1".parse().unwrap();
        assert!(limiter.check_rate_limit(ip).await);
        assert!(limiter.check_rate_limit(ip).await);
        assert!(!limiter.check_rate_limit(ip).await);
        // Wait for refill
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(limiter.check_rate_limit(ip).await);
    }

    #[tokio::test]
    async fn cleanup_removes_stale_entries() {
        let limiter = IpRateLimiter::new(5.0, 5.0);
        let ip: IpAddr = "127.0.0.1".parse().unwrap();
        limiter.check_rate_limit(ip).await;
        assert_eq!(limiter.buckets.lock().await.len(), 1);
        // Cleanup with 0 max_age removes everything
        limiter.cleanup(std::time::Duration::ZERO).await;
        assert_eq!(limiter.buckets.lock().await.len(), 0);
    }
}
