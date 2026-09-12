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
    if !q.lat.is_finite() || !q.lon.is_finite() || q.lat.abs() > 90.0 || q.lon.abs() > 180.0 {
        return Err(axum::http::StatusCode::BAD_REQUEST);
    }
    let cache_key = format!("weather:v2:{:.2}:{:.2}", q.lat, q.lon);
    let url = format!(
        concat!(
            "https://api.open-meteo.com/v1/forecast?latitude={lat}&longitude={lon}",
            "&wind_speed_unit=ms",
            "&current=temperature_2m,relative_humidity_2m,apparent_temperature,precipitation,weather_code,cloud_cover,pressure_msl,wind_speed_10m,wind_direction_10m,wind_gusts_10m",
            "&forecast_days=1"
        ), lat = q.lat, lon = q.lon,
    );

    let raw = match cached_fetch(&state, &cache_key, &url, 900).await {
        Ok(body) => body,
        Err(e) => {
            tracing::error!("Weather fetch error: {e}");

            // Upstream may throttle (429). Serve stale cache instead of hard-failing.
            let key = cache_key.clone();
            let stale = state.cache_db.run(move |conn| {
                let mut stmt = conn.prepare_cached("SELECT body FROM api_cache WHERE key = ?1")?;
                let result = stmt.query_row(rusqlite::params![key], |row| {
                    let blob: Vec<u8> = row.get(0)?;
                    Ok(String::from_utf8_lossy(&blob).into_owned())
                });
                match result {
                    Ok(body) => Ok(Some(body)),
                    Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                    Err(err) => Err(err.into()),
                }
            }).await.unwrap_or(None);

            match stale {
                Some(body) => {
                    tracing::warn!("Serving stale weather cache for {cache_key}");
                    body
                }
                None => return Err(axum::http::StatusCode::BAD_GATEWAY),
            }
        }
    };

    let parsed: serde_json::Value =
        serde_json::from_str(&raw).map_err(|_| axum::http::StatusCode::BAD_GATEWAY)?;

    Ok(Json(parsed))
}
