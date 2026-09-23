//! Cached access to third-party APIs.
//!
//! Every upstream request goes through [`cached`]: fresh cache hits are served
//! directly, identical concurrent misses are coalesced, rate limits put the
//! provider into a cool-down, and failures fall back to the last good response.

use std::future::Future;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::LazyLock;

use crate::db::DbPool;

#[derive(Debug, Clone, PartialEq)]
pub enum UpstreamError {
    RateLimited { retry_after: Option<i64> },
    Status(u16),
    Network(String),
    Invalid(String),
}

impl std::fmt::Display for UpstreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RateLimited { retry_after: Some(s) } => write!(f, "rate limited (retry in {s}s)"),
            Self::RateLimited { retry_after: None } => write!(f, "rate limited"),
            Self::Status(code) => write!(f, "upstream returned HTTP {code}"),
            Self::Network(e) => write!(f, "network error: {e}"),
            Self::Invalid(e) => write!(f, "invalid upstream response: {e}"),
        }
    }
}

impl std::error::Error for UpstreamError {}

/// A cached or freshly fetched response.
#[derive(Debug, Clone)]
pub struct Fetched {
    pub body: Vec<u8>,
    pub fetched_at: i64,
    /// True when the upstream failed and an expired copy is served instead.
    pub stale: bool,
}

/// Per-provider cool-down after rate limiting.
pub struct Provider {
    pub name: &'static str,
    cooldown_until: AtomicI64,
    backoff_secs: AtomicI64,
    min_backoff: i64,
    max_backoff: i64,
}

impl Provider {
    pub const fn new(name: &'static str, min_backoff: i64, max_backoff: i64) -> Self {
        Self { name, cooldown_until: AtomicI64::new(0), backoff_secs: AtomicI64::new(min_backoff), min_backoff, max_backoff }
    }

    pub fn cooling_down(&self, now: i64) -> Option<i64> {
        let until = self.cooldown_until.load(Ordering::Relaxed);
        (now < until).then_some(until - now)
    }

    pub fn on_rate_limit(&self, now: i64, retry_after: Option<i64>) -> i64 {
        let secs = match retry_after {
            Some(s) if s > 0 => s,
            _ => {
                let s = self.backoff_secs.load(Ordering::Relaxed);
                self.backoff_secs.store((s * 2).min(self.max_backoff), Ordering::Relaxed);
                s
            }
        };
        self.cooldown_until.store(now + secs, Ordering::Relaxed);
        tracing::warn!("{} rate limit reached; pausing requests for {secs}s", self.name);
        secs
    }

    pub fn on_success(&self) {
        self.cooldown_until.store(0, Ordering::Relaxed);
        self.backoff_secs.store(self.min_backoff, Ordering::Relaxed);
    }
}

static GATES: LazyLock<Vec<tokio::sync::Mutex<()>>> = LazyLock::new(|| (0..64).map(|_| tokio::sync::Mutex::new(())).collect());

fn gate(key: &str) -> &'static tokio::sync::Mutex<()> {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut hasher);
    &GATES[hasher.finish() as usize % GATES.len()]
}

/// Serve `key` from cache, or run `fetch` and cache the result for `ttl_secs`.
pub async fn cached<F, Fut>(db: &DbPool, provider: &Provider, key: &str, ttl_secs: i64, fetch: F) -> Result<Fetched, UpstreamError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<Vec<u8>, UpstreamError>>,
{
    let _guard = gate(key).lock().await;
    let lookup_key = key.to_string();
    let db_lookup = db.clone();
    let entry = tokio::task::spawn_blocking(move || db_lookup.cache_get(&lookup_key)).await.ok().and_then(|r| r.ok()).flatten();

    if let Some(entry) = &entry {
        if entry.fresh {
            return Ok(Fetched { body: entry.body.clone(), fetched_at: entry.fetched_at, stale: false });
        }
    }
    let stale = |error: UpstreamError| match &entry {
        Some(e) => Ok(Fetched { body: e.body.clone(), fetched_at: e.fetched_at, stale: true }),
        None => Err(error),
    };

    let now = chrono::Utc::now().timestamp();
    if let Some(wait) = provider.cooling_down(now) {
        return stale(UpstreamError::RateLimited { retry_after: Some(wait) });
    }

    match fetch().await {
        Ok(body) => {
            provider.on_success();
            let (db_store, store_key, store_body) = (db.clone(), key.to_string(), body.clone());
            let _ = tokio::task::spawn_blocking(move || db_store.cache_set(&store_key, &store_body, ttl_secs)).await;
            Ok(Fetched { body, fetched_at: now, stale: false })
        }
        Err(error) => {
            if let UpstreamError::RateLimited { retry_after } = &error {
                provider.on_rate_limit(now, *retry_after);
            } else {
                tracing::warn!("{} request failed: {error}", provider.name);
            }
            stale(error)
        }
    }
}

/// Send a request and return the body, mapping HTTP errors to [`UpstreamError`].
pub async fn send(request: reqwest::RequestBuilder) -> Result<Vec<u8>, UpstreamError> {
    let response = request.send().await.map_err(|e| UpstreamError::Network(without_query(&e.to_string())))?;
    let status = response.status();
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        let retry_after = ["x-rate-limit-retry-after-seconds", "retry-after"]
            .iter()
            .find_map(|h| response.headers().get(*h)?.to_str().ok()?.trim().parse::<i64>().ok());
        return Err(UpstreamError::RateLimited { retry_after });
    }
    if !status.is_success() {
        return Err(UpstreamError::Status(status.as_u16()));
    }
    response.bytes().await.map(|b| b.to_vec()).map_err(|e| UpstreamError::Network(without_query(&e.to_string())))
}

/// reqwest errors include the URL; strip query strings so API keys never reach logs.
fn without_query(message: &str) -> String {
    let mut out = String::with_capacity(message.len());
    let mut skipping = false;
    for c in message.chars() {
        if c == '?' {
            skipping = true;
            out.push_str("?…");
        } else if skipping && (c == ')' || c.is_whitespace()) {
            skipping = false;
        }
        if !skipping {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn serves_fresh_cache_then_stale_on_failure() {
        let db = DbPool::open_cache(":memory:").unwrap();
        let provider = Provider::new("test", 10, 100);
        let first = cached(&db, &provider, "k", 60, || async { Ok(b"one".to_vec()) }).await.unwrap();
        assert_eq!(first.body, b"one");
        let hit = cached(&db, &provider, "k", 60, || async { panic!("must not fetch") }).await.unwrap();
        assert!(!hit.stale);

        db.conn().execute("UPDATE api_cache SET fetched_at = fetched_at - 3600", []).unwrap();
        let fallback = cached(&db, &provider, "k", 60, || async { Err(UpstreamError::Status(500)) }).await.unwrap();
        assert!(fallback.stale);
        assert_eq!(fallback.body, b"one");

        let missing = cached(&db, &provider, "other", 60, || async { Err(UpstreamError::Status(502)) }).await;
        assert_eq!(missing.unwrap_err(), UpstreamError::Status(502));
    }

    #[tokio::test]
    async fn rate_limits_pause_the_provider() {
        let db = DbPool::open_cache(":memory:").unwrap();
        let provider = Provider::new("test", 10, 100);
        let err = cached(&db, &provider, "a", 60, || async { Err(UpstreamError::RateLimited { retry_after: Some(30) }) }).await;
        assert!(matches!(err, Err(UpstreamError::RateLimited { .. })));
        let paused = cached(&db, &provider, "b", 60, || async { panic!("provider is cooling down") }).await;
        assert!(matches!(paused, Err(UpstreamError::RateLimited { retry_after: Some(s) }) if s > 0 && s <= 30));
        provider.on_success();
        assert!(provider.cooling_down(chrono::Utc::now().timestamp()).is_none());
    }

    #[test]
    fn exponential_backoff_is_bounded() {
        let provider = Provider::new("test", 10, 40);
        assert_eq!(provider.on_rate_limit(0, None), 10);
        assert_eq!(provider.on_rate_limit(0, None), 20);
        assert_eq!(provider.on_rate_limit(0, None), 40);
        assert_eq!(provider.on_rate_limit(0, None), 40);
        assert_eq!(provider.on_rate_limit(0, Some(5)), 5);
    }

    #[test]
    fn api_keys_are_removed_from_error_messages() {
        let msg = without_query("error sending request for url (https://api.tomtom.com/x.png?key=SECRET)");
        assert!(!msg.contains("SECRET"), "{msg}");
        assert!(msg.contains("api.tomtom.com/x.png?…)"));
    }
}
