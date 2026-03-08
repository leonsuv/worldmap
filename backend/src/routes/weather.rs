use axum::{extract::{Query, State}, Json};
use serde::Deserialize;
use std::sync::Arc;

use crate::cache_proxy::cached_fetch;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct WeatherQuery {
    lat: f64,
    lon: f64,
}

pub async fn get_weather(
    State(state): State<Arc<AppState>>,
    Query(q): Query<WeatherQuery>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let cache_key = format!("weather:{:.2}:{:.2}", q.lat, q.lon);
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}&hourly=windspeed_10m,winddirection_10m,wave_height,wave_direction&forecast_days=1&windspeed_unit=ms",
        q.lat, q.lon
    );

    let raw = cached_fetch(&state, &cache_key, &url, 900)
        .await
        .map_err(|e| {
            tracing::error!("Weather fetch error: {e}");
            axum::http::StatusCode::BAD_GATEWAY
        })?;

    let parsed: serde_json::Value =
        serde_json::from_str(&raw).map_err(|_| axum::http::StatusCode::BAD_GATEWAY)?;

    Ok(Json(parsed))
}
