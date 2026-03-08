use crate::state::AppState;
use anyhow::Result;
use std::sync::Arc;

pub async fn cached_fetch(
    state: &Arc<AppState>,
    cache_key: &str,
    url: &str,
    ttl_secs: i64,
) -> Result<String> {
    if let Some(cached) = state.cache_db.cache_get(cache_key)? {
        return Ok(cached);
    }

    let resp = state
        .http_client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;

    state.cache_db.cache_set(cache_key, &resp, ttl_secs)?;
    Ok(resp)
}
