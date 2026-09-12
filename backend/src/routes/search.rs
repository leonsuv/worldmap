use axum::{extract::{Query, State}, http::StatusCode, Json};
use serde::Deserialize;
use std::sync::Arc;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct SearchQuery { q: String }

// Serialize geocoder requests across clients and respect its one-request/second limit.
static SEARCH_GATE: tokio::sync::Mutex<Option<std::time::Instant>> = tokio::sync::Mutex::const_new(None);

pub async fn search(State(state): State<Arc<AppState>>, Query(query): Query<SearchQuery>) -> Result<Json<serde_json::Value>, StatusCode> {
    let query = query.q.trim().to_owned();
    if query.chars().count() < 2 || query.len() > 200 { return Err(StatusCode::BAD_REQUEST); }
    let key = format!("search:{}", query.to_lowercase());
    let mut last = SEARCH_GATE.lock().await;
    let db = state.cache_db.clone();
    let cache_key = key.clone();
    let cached = tokio::task::spawn_blocking(move || db.cache_get(&cache_key)).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if let Some(body) = cached {
        return serde_json::from_str(&body).map(Json).map_err(|_| StatusCode::BAD_GATEWAY);
    }
    if let Some(at) = *last {
        let elapsed = at.elapsed();
        if elapsed < std::time::Duration::from_secs(1) { tokio::time::sleep(std::time::Duration::from_secs(1) - elapsed).await; }
    }
    *last = Some(std::time::Instant::now());
    let response = state.http_client.get("https://nominatim.openstreetmap.org/search")
        .header("User-Agent", "WorldMap-Infrastructure-Explorer/0.1 (https://github.com/leobak/worldmap)")
        .query(&[("q", query.as_str()), ("format", "json"), ("limit", "5"), ("accept-language", "en")])
        .send().await.map_err(|_| StatusCode::BAD_GATEWAY)?
        .error_for_status().map_err(|_| StatusCode::BAD_GATEWAY)?;
    let data: serde_json::Value = response.json().await.map_err(|_| StatusCode::BAD_GATEWAY)?;
    if !data.is_array() { return Err(StatusCode::BAD_GATEWAY); }
    let db = state.cache_db.clone();
    let body = data.to_string();
    tokio::task::spawn_blocking(move || db.cache_set(&key, &body, 86400)).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(data))
}

pub async fn status(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "tiles": state.tile_index.source_names(),
        "ships_configured": std::env::var("AISSTREAM_API_KEY").is_ok_and(|key| !key.trim().is_empty()),
        "traffic_configured": std::env::var("TOMTOM_API_KEY").is_ok_and(|key| !key.trim().is_empty()),
    }))
}
