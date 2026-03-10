use crate::state::AppState;
use anyhow::Result;
use std::sync::Arc;

pub async fn cached_fetch(
    state: &Arc<AppState>,
    cache_key: &str,
    url: &str,
    ttl_secs: i64,
) -> Result<String> {
    // Check cache on blocking thread to avoid holding std::sync::Mutex on async runtime
    let db = state.cache_db.clone();
    let key = cache_key.to_string();
    let cached = tokio::task::spawn_blocking(move || db.cache_get(&key)).await??;
    if let Some(body) = cached {
        return Ok(body);
    }

    let resp = state
        .http_client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;

    let db = state.cache_db.clone();
    let key = cache_key.to_string();
    let body = resp.clone();
    tokio::task::spawn_blocking(move || db.cache_set(&key, &body, ttl_secs)).await??;
    Ok(resp)
}
