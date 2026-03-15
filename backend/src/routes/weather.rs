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
