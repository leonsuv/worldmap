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
    let center = bbox_to_center(&q.bbox).ok_or(axum::http::StatusCode::BAD_REQUEST)?;
    let tomtom_key = std::env::var("TOMTOM_API_KEY").ok().filter(|key| !key.trim().is_empty()).ok_or(()).map_err(|_| {
        tracing::error!("TOMTOM_API_KEY not set");
        axum::http::StatusCode::SERVICE_UNAVAILABLE
    })?;

    let cache_key = format!("traffic:{}", q.bbox);
    // TomTom Traffic Flow Segment Data (free tier: 2500 req/day)
    let url = format!(
        "https://api.tomtom.com/traffic/services/4/flowSegmentData/absolute/10/json?point={}&key={}",
        center, tomtom_key
    );

    let raw = cached_fetch(&state, &cache_key, &url, 60)
        .await
        .map_err(|_| {
            tracing::error!("Traffic provider request failed");
            axum::http::StatusCode::BAD_GATEWAY
        })?;

    let parsed: serde_json::Value =
        serde_json::from_str(&raw).map_err(|_| axum::http::StatusCode::BAD_GATEWAY)?;

    Ok(Json(parsed))
}

/// Convert "west,south,east,north" bbox to a center "lat,lon" string for TomTom.
fn bbox_to_center(bbox: &str) -> Option<String> {
    let parts: Vec<f64> = bbox.split(',').map(|s| s.trim().parse::<f64>()).collect::<Result<_, _>>().ok()?;
    if parts.len() != 4 || parts.iter().any(|v| !v.is_finite()) { return None; }
    let (west, south, east, north) = (parts[0], parts[1], parts[2], parts[3]);
    if south < -90.0 || north > 90.0 || south > north || (east - west).abs() > 360.0 { return None; }
    let lat = (south + north) / 2.0;
    // MapLibre can return wrapped longitudes; support the antimeridian too.
    let span = (east - west).rem_euclid(360.0);
    let lon = (west + span / 2.0 + 180.0).rem_euclid(360.0) - 180.0;
    Some(format!("{lat},{lon}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_bad_traffic_bounds_instead_of_querying_zero_zero() {
        for bbox in ["", "x,1,2,3,4", "1,2,3", "0,0,NaN,1", "0,-91,1,2", "0,4,1,2", "0,0,361,1"] {
            assert!(bbox_to_center(bbox).is_none(), "{bbox}");
        }
    }
    #[test]
    fn centers_normal_and_wrapped_traffic_bounds() {
        assert_eq!(bbox_to_center("10,50,14,54"), Some("52,12".into()));
        assert_eq!(bbox_to_center("170,-10,-170,10"), Some("0,-180".into()));
        assert_eq!(bbox_to_center("350,50,370,54"), Some("52,0".into()));
    }
}
