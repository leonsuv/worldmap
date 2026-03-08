use axum::{extract::{Query, State}, Json};
use serde::Deserialize;
use std::sync::Arc;

use crate::cache_proxy::cached_fetch;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct TrafficQuery {
    bbox: String, // "west,south,east,north"  →  TomTom expects "minLon,minLat,maxLon,maxLat"
}

pub async fn get_traffic(
    State(state): State<Arc<AppState>>,
    Query(q): Query<TrafficQuery>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let tomtom_key = std::env::var("TOMTOM_API_KEY").map_err(|_| {
        tracing::error!("TOMTOM_API_KEY not set");
        axum::http::StatusCode::SERVICE_UNAVAILABLE
    })?;

    let cache_key = format!("traffic:{}", q.bbox);
    // TomTom Traffic Flow Segment Data (free tier: 2500 req/day)
    let url = format!(
        "https://api.tomtom.com/traffic/services/4/flowSegmentData/absolute/10/json?point={}&key={}",
        bbox_to_center(&q.bbox), tomtom_key
    );

    let raw = cached_fetch(&state, &cache_key, &url, 60)
        .await
        .map_err(|e| {
            tracing::error!("Traffic fetch error: {e}");
            axum::http::StatusCode::BAD_GATEWAY
        })?;

    let parsed: serde_json::Value =
        serde_json::from_str(&raw).map_err(|_| axum::http::StatusCode::BAD_GATEWAY)?;

    Ok(Json(parsed))
}

/// Convert "west,south,east,north" bbox to a center "lat,lon" string for TomTom.
fn bbox_to_center(bbox: &str) -> String {
    let parts: Vec<f64> = bbox.split(',').filter_map(|s| s.trim().parse().ok()).collect();
    if parts.len() == 4 {
        let lat = (parts[1] + parts[3]) / 2.0;
        let lon = (parts[0] + parts[2]) / 2.0;
        format!("{lat},{lon}")
    } else {
        "0,0".to_string()
    }
}
